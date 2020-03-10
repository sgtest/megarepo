//! The implementation of the query system itself. This defines the macros that
//! generate the actual methods on tcx which find and execute the provider,
//! manage the caches, and so forth.

use crate::dep_graph::{DepNode, DepNodeIndex, SerializedDepNodeIndex};
use crate::ty::query::caches::QueryCache;
use crate::ty::query::config::{QueryAccessors, QueryDescription};
use crate::ty::query::job::{QueryInfo, QueryJob, QueryJobId, QueryShardJobId};
use crate::ty::query::Query;
use crate::ty::tls;
use crate::ty::{self, TyCtxt};

#[cfg(not(parallel_compiler))]
use rustc_data_structures::cold_path;
use rustc_data_structures::fx::{FxHashMap, FxHasher};
use rustc_data_structures::sharded::Sharded;
use rustc_data_structures::sync::{Lock, LockGuard};
use rustc_data_structures::thin_vec::ThinVec;
use rustc_errors::{struct_span_err, Diagnostic, DiagnosticBuilder, FatalError, Handler, Level};
use rustc_span::source_map::DUMMY_SP;
use rustc_span::Span;
use std::collections::hash_map::Entry;
use std::hash::{Hash, Hasher};
use std::mem;
use std::num::NonZeroU32;
use std::ptr;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct QueryStateShard<'tcx, D: QueryAccessors<'tcx> + ?Sized> {
    pub(super) cache: <<D as QueryAccessors<'tcx>>::Cache as QueryCache<D::Key, D::Value>>::Sharded,
    pub(super) active: FxHashMap<D::Key, QueryResult<'tcx>>,

    /// Used to generate unique ids for active jobs.
    pub(super) jobs: u32,
}

impl<'tcx, Q: QueryAccessors<'tcx>> QueryStateShard<'tcx, Q> {
    fn get_cache(
        &mut self,
    ) -> &mut <<Q as QueryAccessors<'tcx>>::Cache as QueryCache<Q::Key, Q::Value>>::Sharded {
        &mut self.cache
    }
}

impl<'tcx, Q: QueryAccessors<'tcx>> Default for QueryStateShard<'tcx, Q> {
    fn default() -> QueryStateShard<'tcx, Q> {
        QueryStateShard { cache: Default::default(), active: Default::default(), jobs: 0 }
    }
}

pub(crate) struct QueryState<'tcx, D: QueryAccessors<'tcx> + ?Sized> {
    pub(super) cache: D::Cache,
    pub(super) shards: Sharded<QueryStateShard<'tcx, D>>,
    #[cfg(debug_assertions)]
    pub(super) cache_hits: AtomicUsize,
}

impl<'tcx, Q: QueryAccessors<'tcx>> QueryState<'tcx, Q> {
    pub(super) fn get_lookup<K: Hash>(&'tcx self, key: &K) -> QueryLookup<'tcx, Q> {
        // We compute the key's hash once and then use it for both the
        // shard lookup and the hashmap lookup. This relies on the fact
        // that both of them use `FxHasher`.
        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        let key_hash = hasher.finish();

        let shard = self.shards.get_shard_index_by_hash(key_hash);
        let lock = self.shards.get_shard_by_index(shard).lock();
        QueryLookup { key_hash, shard, lock }
    }
}

/// Indicates the state of a query for a given key in a query map.
pub(super) enum QueryResult<'tcx> {
    /// An already executing query. The query job can be used to await for its completion.
    Started(QueryJob<'tcx>),

    /// The query panicked. Queries trying to wait on this will raise a fatal error which will
    /// silently panic.
    Poisoned,
}

impl<'tcx, M: QueryAccessors<'tcx>> QueryState<'tcx, M> {
    pub fn iter_results<R>(
        &self,
        f: impl for<'a> FnOnce(
            Box<dyn Iterator<Item = (&'a M::Key, &'a M::Value, DepNodeIndex)> + 'a>,
        ) -> R,
    ) -> R {
        self.cache.iter(&self.shards, |shard| &mut shard.cache, f)
    }
    pub fn all_inactive(&self) -> bool {
        let shards = self.shards.lock_shards();
        shards.iter().all(|shard| shard.active.is_empty())
    }
}

impl<'tcx, M: QueryAccessors<'tcx>> Default for QueryState<'tcx, M> {
    fn default() -> QueryState<'tcx, M> {
        QueryState {
            cache: M::Cache::default(),
            shards: Default::default(),
            #[cfg(debug_assertions)]
            cache_hits: AtomicUsize::new(0),
        }
    }
}

/// Values used when checking a query cache which can be reused on a cache-miss to execute the query.
pub(crate) struct QueryLookup<'tcx, Q: QueryAccessors<'tcx>> {
    pub(super) key_hash: u64,
    pub(super) shard: usize,
    pub(super) lock: LockGuard<'tcx, QueryStateShard<'tcx, Q>>,
}

/// A type representing the responsibility to execute the job in the `job` field.
/// This will poison the relevant query if dropped.
pub(super) struct JobOwner<'tcx, Q: QueryDescription<'tcx>> {
    tcx: TyCtxt<'tcx>,
    key: Q::Key,
    id: QueryJobId,
}

impl<'tcx, Q: QueryDescription<'tcx>> JobOwner<'tcx, Q> {
    /// Either gets a `JobOwner` corresponding the query, allowing us to
    /// start executing the query, or returns with the result of the query.
    /// This function assumes that `try_get_cached` is already called and returned `lookup`.
    /// If the query is executing elsewhere, this will wait for it and return the result.
    /// If the query panicked, this will silently panic.
    ///
    /// This function is inlined because that results in a noticeable speed-up
    /// for some compile-time benchmarks.
    #[inline(always)]
    pub(super) fn try_start(
        tcx: TyCtxt<'tcx>,
        span: Span,
        key: &Q::Key,
        mut lookup: QueryLookup<'tcx, Q>,
    ) -> TryGetJob<'tcx, Q> {
        let lock = &mut *lookup.lock;

        let (latch, mut _query_blocked_prof_timer) = match lock.active.entry((*key).clone()) {
            Entry::Occupied(mut entry) => {
                match entry.get_mut() {
                    QueryResult::Started(job) => {
                        // For parallel queries, we'll block and wait until the query running
                        // in another thread has completed. Record how long we wait in the
                        // self-profiler.
                        let _query_blocked_prof_timer = if cfg!(parallel_compiler) {
                            Some(tcx.prof.query_blocked())
                        } else {
                            None
                        };

                        // Create the id of the job we're waiting for
                        let id = QueryJobId::new(job.id, lookup.shard, Q::dep_kind());

                        (job.latch(id), _query_blocked_prof_timer)
                    }
                    QueryResult::Poisoned => FatalError.raise(),
                }
            }
            Entry::Vacant(entry) => {
                // No job entry for this query. Return a new one to be started later.

                // Generate an id unique within this shard.
                let id = lock.jobs.checked_add(1).unwrap();
                lock.jobs = id;
                let id = QueryShardJobId(NonZeroU32::new(id).unwrap());

                let global_id = QueryJobId::new(id, lookup.shard, Q::dep_kind());

                let job = tls::with_related_context(tcx, |icx| QueryJob::new(id, span, icx.query));

                entry.insert(QueryResult::Started(job));

                let owner = JobOwner { tcx, id: global_id, key: (*key).clone() };
                return TryGetJob::NotYetStarted(owner);
            }
        };
        mem::drop(lookup.lock);

        // If we are single-threaded we know that we have cycle error,
        // so we just return the error.
        #[cfg(not(parallel_compiler))]
        return TryGetJob::Cycle(cold_path(|| {
            Q::handle_cycle_error(tcx, latch.find_cycle_in_stack(tcx, span))
        }));

        // With parallel queries we might just have to wait on some other
        // thread.
        #[cfg(parallel_compiler)]
        {
            let result = latch.wait_on(tcx, span);

            if let Err(cycle) = result {
                return TryGetJob::Cycle(Q::handle_cycle_error(tcx, cycle));
            }

            let cached = tcx.try_get_cached::<Q, _, _, _>(
                (*key).clone(),
                |value, index| (value.clone(), index),
                |_, _| panic!("value must be in cache after waiting"),
            );

            if let Some(prof_timer) = _query_blocked_prof_timer.take() {
                prof_timer.finish_with_query_invocation_id(cached.1.into());
            }

            return TryGetJob::JobCompleted(cached);
        }
    }

    /// Completes the query by updating the query cache with the `result`,
    /// signals the waiter and forgets the JobOwner, so it won't poison the query
    #[inline(always)]
    pub(super) fn complete(self, result: &Q::Value, dep_node_index: DepNodeIndex) {
        // We can move out of `self` here because we `mem::forget` it below
        let key = unsafe { ptr::read(&self.key) };
        let tcx = self.tcx;

        // Forget ourself so our destructor won't poison the query
        mem::forget(self);

        let job = {
            let state = Q::query_state(tcx);
            let result = result.clone();
            let mut lock = state.shards.get_shard_by_value(&key).lock();
            let job = match lock.active.remove(&key).unwrap() {
                QueryResult::Started(job) => job,
                QueryResult::Poisoned => panic!(),
            };
            state.cache.complete(tcx, &mut lock.cache, key, result, dep_node_index);
            job
        };

        job.signal_complete();
    }
}

#[inline(always)]
fn with_diagnostics<F, R>(f: F) -> (R, ThinVec<Diagnostic>)
where
    F: FnOnce(Option<&Lock<ThinVec<Diagnostic>>>) -> R,
{
    let diagnostics = Lock::new(ThinVec::new());
    let result = f(Some(&diagnostics));
    (result, diagnostics.into_inner())
}

impl<'tcx, Q: QueryDescription<'tcx>> Drop for JobOwner<'tcx, Q> {
    #[inline(never)]
    #[cold]
    fn drop(&mut self) {
        // Poison the query so jobs waiting on it panic.
        let state = Q::query_state(self.tcx);
        let shard = state.shards.get_shard_by_value(&self.key);
        let job = {
            let mut shard = shard.lock();
            let job = match shard.active.remove(&self.key).unwrap() {
                QueryResult::Started(job) => job,
                QueryResult::Poisoned => panic!(),
            };
            shard.active.insert(self.key.clone(), QueryResult::Poisoned);
            job
        };
        // Also signal the completion of the job, so waiters
        // will continue execution.
        job.signal_complete();
    }
}

#[derive(Clone)]
pub struct CycleError<'tcx> {
    /// The query and related span that uses the cycle.
    pub(super) usage: Option<(Span, Query<'tcx>)>,
    pub(super) cycle: Vec<QueryInfo<'tcx>>,
}

/// The result of `try_start`.
pub(super) enum TryGetJob<'tcx, D: QueryDescription<'tcx>> {
    /// The query is not yet started. Contains a guard to the cache eventually used to start it.
    NotYetStarted(JobOwner<'tcx, D>),

    /// The query was already completed.
    /// Returns the result of the query and its dep-node index
    /// if it succeeded or a cycle error if it failed.
    #[cfg(parallel_compiler)]
    JobCompleted((D::Value, DepNodeIndex)),

    /// Trying to execute the query resulted in a cycle.
    Cycle(D::Value),
}

impl<'tcx> TyCtxt<'tcx> {
    /// Executes a job by changing the `ImplicitCtxt` to point to the
    /// new query job while it executes. It returns the diagnostics
    /// captured during execution and the actual result.
    #[inline(always)]
    pub(super) fn start_query<F, R>(
        self,
        token: QueryJobId,
        diagnostics: Option<&Lock<ThinVec<Diagnostic>>>,
        compute: F,
    ) -> R
    where
        F: FnOnce(TyCtxt<'tcx>) -> R,
    {
        // The `TyCtxt` stored in TLS has the same global interner lifetime
        // as `self`, so we use `with_related_context` to relate the 'tcx lifetimes
        // when accessing the `ImplicitCtxt`.
        tls::with_related_context(self, move |current_icx| {
            // Update the `ImplicitCtxt` to point to our new query job.
            let new_icx = tls::ImplicitCtxt {
                tcx: self,
                query: Some(token),
                diagnostics,
                layout_depth: current_icx.layout_depth,
                task_deps: current_icx.task_deps,
            };

            // Use the `ImplicitCtxt` while we execute the query.
            tls::enter_context(&new_icx, |_| compute(self))
        })
    }

    #[inline(never)]
    #[cold]
    pub(super) fn report_cycle(
        self,
        CycleError { usage, cycle: stack }: CycleError<'tcx>,
    ) -> DiagnosticBuilder<'tcx> {
        assert!(!stack.is_empty());

        let fix_span = |span: Span, query: &Query<'tcx>| {
            self.sess.source_map().def_span(query.default_span(self, span))
        };

        // Disable naming impls with types in this path, since that
        // sometimes cycles itself, leading to extra cycle errors.
        // (And cycle errors around impls tend to occur during the
        // collect/coherence phases anyhow.)
        ty::print::with_forced_impl_filename_line(|| {
            let span = fix_span(stack[1 % stack.len()].span, &stack[0].query);
            let mut err = struct_span_err!(
                self.sess,
                span,
                E0391,
                "cycle detected when {}",
                stack[0].query.describe(self)
            );

            for i in 1..stack.len() {
                let query = &stack[i].query;
                let span = fix_span(stack[(i + 1) % stack.len()].span, query);
                err.span_note(span, &format!("...which requires {}...", query.describe(self)));
            }

            err.note(&format!(
                "...which again requires {}, completing the cycle",
                stack[0].query.describe(self)
            ));

            if let Some((span, query)) = usage {
                err.span_note(
                    fix_span(span, &query),
                    &format!("cycle used when {}", query.describe(self)),
                );
            }

            err
        })
    }

    pub fn try_print_query_stack(handler: &Handler) {
        eprintln!("query stack during panic:");

        // Be careful reyling on global state here: this code is called from
        // a panic hook, which means that the global `Handler` may be in a weird
        // state if it was responsible for triggering the panic.
        tls::with_context_opt(|icx| {
            if let Some(icx) = icx {
                let query_map = icx.tcx.queries.try_collect_active_jobs();

                let mut current_query = icx.query;
                let mut i = 0;

                while let Some(query) = current_query {
                    let query_info =
                        if let Some(info) = query_map.as_ref().and_then(|map| map.get(&query)) {
                            info
                        } else {
                            break;
                        };
                    let mut diag = Diagnostic::new(
                        Level::FailureNote,
                        &format!(
                            "#{} [{}] {}",
                            i,
                            query_info.info.query.name(),
                            query_info.info.query.describe(icx.tcx)
                        ),
                    );
                    diag.span = icx.tcx.sess.source_map().def_span(query_info.info.span).into();
                    handler.force_print_diagnostic(diag);

                    current_query = query_info.job.parent;
                    i += 1;
                }
            }
        });

        eprintln!("end of query stack");
    }

    /// Checks if the query is already computed and in the cache.
    /// It returns the shard index and a lock guard to the shard,
    /// which will be used if the query is not in the cache and we need
    /// to compute it.
    #[inline(always)]
    fn try_get_cached<Q, R, OnHit, OnMiss>(
        self,
        key: Q::Key,
        // `on_hit` can be called while holding a lock to the query cache
        on_hit: OnHit,
        on_miss: OnMiss,
    ) -> R
    where
        Q: QueryDescription<'tcx> + 'tcx,
        OnHit: FnOnce(&Q::Value, DepNodeIndex) -> R,
        OnMiss: FnOnce(Q::Key, QueryLookup<'tcx, Q>) -> R,
    {
        let state = Q::query_state(self);

        state.cache.lookup(
            state,
            QueryStateShard::<Q>::get_cache,
            key,
            |value, index| {
                if unlikely!(self.prof.enabled()) {
                    self.prof.query_cache_hit(index.into());
                }
                #[cfg(debug_assertions)]
                {
                    state.cache_hits.fetch_add(1, Ordering::Relaxed);
                }
                on_hit(value, index)
            },
            on_miss,
        )
    }

    #[inline(never)]
    pub(super) fn get_query<Q: QueryDescription<'tcx> + 'tcx>(
        self,
        span: Span,
        key: Q::Key,
    ) -> Q::Value {
        debug!("ty::query::get_query<{}>(key={:?}, span={:?})", Q::NAME, key, span);

        self.try_get_cached::<Q, _, _, _>(
            key,
            |value, index| {
                self.dep_graph.read_index(index);
                value.clone()
            },
            |key, lookup| self.try_execute_query::<Q>(span, key, lookup),
        )
    }

    #[inline(always)]
    pub(super) fn try_execute_query<Q: QueryDescription<'tcx>>(
        self,
        span: Span,
        key: Q::Key,
        lookup: QueryLookup<'tcx, Q>,
    ) -> Q::Value {
        let job = match JobOwner::try_start(self, span, &key, lookup) {
            TryGetJob::NotYetStarted(job) => job,
            TryGetJob::Cycle(result) => return result,
            #[cfg(parallel_compiler)]
            TryGetJob::JobCompleted((v, index)) => {
                self.dep_graph.read_index(index);
                return v;
            }
        };

        // Fast path for when incr. comp. is off. `to_dep_node` is
        // expensive for some `DepKind`s.
        if !self.dep_graph.is_fully_enabled() {
            let null_dep_node = DepNode::new_no_params(crate::dep_graph::DepKind::Null);
            return self.force_query_with_job::<Q>(key, job, null_dep_node).0;
        }

        if Q::ANON {
            let prof_timer = self.prof.query_provider();

            let ((result, dep_node_index), diagnostics) = with_diagnostics(|diagnostics| {
                self.start_query(job.id, diagnostics, |tcx| {
                    tcx.dep_graph.with_anon_task(Q::dep_kind(), || Q::compute(tcx, key))
                })
            });

            prof_timer.finish_with_query_invocation_id(dep_node_index.into());

            self.dep_graph.read_index(dep_node_index);

            if unlikely!(!diagnostics.is_empty()) {
                self.queries
                    .on_disk_cache
                    .store_diagnostics_for_anon_node(dep_node_index, diagnostics);
            }

            job.complete(&result, dep_node_index);

            return result;
        }

        let dep_node = Q::to_dep_node(self, &key);

        if !Q::EVAL_ALWAYS {
            // The diagnostics for this query will be
            // promoted to the current session during
            // `try_mark_green()`, so we can ignore them here.
            let loaded = self.start_query(job.id, None, |tcx| {
                let marked = tcx.dep_graph.try_mark_green_and_read(tcx, &dep_node);
                marked.map(|(prev_dep_node_index, dep_node_index)| {
                    (
                        tcx.load_from_disk_and_cache_in_memory::<Q>(
                            key.clone(),
                            prev_dep_node_index,
                            dep_node_index,
                            &dep_node,
                        ),
                        dep_node_index,
                    )
                })
            });
            if let Some((result, dep_node_index)) = loaded {
                job.complete(&result, dep_node_index);
                return result;
            }
        }

        let (result, dep_node_index) = self.force_query_with_job::<Q>(key, job, dep_node);
        self.dep_graph.read_index(dep_node_index);
        result
    }

    fn load_from_disk_and_cache_in_memory<Q: QueryDescription<'tcx>>(
        self,
        key: Q::Key,
        prev_dep_node_index: SerializedDepNodeIndex,
        dep_node_index: DepNodeIndex,
        dep_node: &DepNode,
    ) -> Q::Value {
        // Note this function can be called concurrently from the same query
        // We must ensure that this is handled correctly.

        debug_assert!(self.dep_graph.is_green(dep_node));

        // First we try to load the result from the on-disk cache.
        let result = if Q::cache_on_disk(self, key.clone(), None)
            && self.sess.opts.debugging_opts.incremental_queries
        {
            let prof_timer = self.prof.incr_cache_loading();
            let result = Q::try_load_from_disk(self, prev_dep_node_index);
            prof_timer.finish_with_query_invocation_id(dep_node_index.into());

            // We always expect to find a cached result for things that
            // can be forced from `DepNode`.
            debug_assert!(
                !dep_node.kind.can_reconstruct_query_key() || result.is_some(),
                "missing on-disk cache entry for {:?}",
                dep_node
            );
            result
        } else {
            // Some things are never cached on disk.
            None
        };

        let result = if let Some(result) = result {
            result
        } else {
            // We could not load a result from the on-disk cache, so
            // recompute.
            let prof_timer = self.prof.query_provider();

            // The dep-graph for this computation is already in-place.
            let result = self.dep_graph.with_ignore(|| Q::compute(self, key));

            prof_timer.finish_with_query_invocation_id(dep_node_index.into());

            result
        };

        // If `-Zincremental-verify-ich` is specified, re-hash results from
        // the cache and make sure that they have the expected fingerprint.
        if unlikely!(self.sess.opts.debugging_opts.incremental_verify_ich) {
            self.incremental_verify_ich::<Q>(&result, dep_node, dep_node_index);
        }

        result
    }

    #[inline(never)]
    #[cold]
    fn incremental_verify_ich<Q: QueryDescription<'tcx>>(
        self,
        result: &Q::Value,
        dep_node: &DepNode,
        dep_node_index: DepNodeIndex,
    ) {
        use crate::ich::Fingerprint;

        assert!(
            Some(self.dep_graph.fingerprint_of(dep_node_index))
                == self.dep_graph.prev_fingerprint_of(dep_node),
            "fingerprint for green query instance not loaded from cache: {:?}",
            dep_node,
        );

        debug!("BEGIN verify_ich({:?})", dep_node);
        let mut hcx = self.create_stable_hashing_context();

        let new_hash = Q::hash_result(&mut hcx, result).unwrap_or(Fingerprint::ZERO);
        debug!("END verify_ich({:?})", dep_node);

        let old_hash = self.dep_graph.fingerprint_of(dep_node_index);

        assert!(new_hash == old_hash, "found unstable fingerprints for {:?}", dep_node,);
    }

    #[inline(always)]
    fn force_query_with_job<Q: QueryDescription<'tcx>>(
        self,
        key: Q::Key,
        job: JobOwner<'tcx, Q>,
        dep_node: DepNode,
    ) -> (Q::Value, DepNodeIndex) {
        // If the following assertion triggers, it can have two reasons:
        // 1. Something is wrong with DepNode creation, either here or
        //    in `DepGraph::try_mark_green()`.
        // 2. Two distinct query keys get mapped to the same `DepNode`
        //    (see for example #48923).
        assert!(
            !self.dep_graph.dep_node_exists(&dep_node),
            "forcing query with already existing `DepNode`\n\
                 - query-key: {:?}\n\
                 - dep-node: {:?}",
            key,
            dep_node
        );

        let prof_timer = self.prof.query_provider();

        let ((result, dep_node_index), diagnostics) = with_diagnostics(|diagnostics| {
            self.start_query(job.id, diagnostics, |tcx| {
                if Q::EVAL_ALWAYS {
                    tcx.dep_graph.with_eval_always_task(
                        dep_node,
                        tcx,
                        key,
                        Q::compute,
                        Q::hash_result,
                    )
                } else {
                    tcx.dep_graph.with_task(dep_node, tcx, key, Q::compute, Q::hash_result)
                }
            })
        });

        prof_timer.finish_with_query_invocation_id(dep_node_index.into());

        if unlikely!(!diagnostics.is_empty()) {
            if dep_node.kind != crate::dep_graph::DepKind::Null {
                self.queries.on_disk_cache.store_diagnostics(dep_node_index, diagnostics);
            }
        }

        job.complete(&result, dep_node_index);

        (result, dep_node_index)
    }

    /// Ensure that either this query has all green inputs or been executed.
    /// Executing `query::ensure(D)` is considered a read of the dep-node `D`.
    ///
    /// This function is particularly useful when executing passes for their
    /// side-effects -- e.g., in order to report errors for erroneous programs.
    ///
    /// Note: The optimization is only available during incr. comp.
    pub(super) fn ensure_query<Q: QueryDescription<'tcx> + 'tcx>(self, key: Q::Key) -> () {
        if Q::EVAL_ALWAYS {
            let _ = self.get_query::<Q>(DUMMY_SP, key);
            return;
        }

        // Ensuring an anonymous query makes no sense
        assert!(!Q::ANON);

        let dep_node = Q::to_dep_node(self, &key);

        match self.dep_graph.try_mark_green_and_read(self, &dep_node) {
            None => {
                // A None return from `try_mark_green_and_read` means that this is either
                // a new dep node or that the dep node has already been marked red.
                // Either way, we can't call `dep_graph.read()` as we don't have the
                // DepNodeIndex. We must invoke the query itself. The performance cost
                // this introduces should be negligible as we'll immediately hit the
                // in-memory cache, or another query down the line will.
                let _ = self.get_query::<Q>(DUMMY_SP, key);
            }
            Some((_, dep_node_index)) => {
                self.prof.query_cache_hit(dep_node_index.into());
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn force_query<Q: QueryDescription<'tcx> + 'tcx>(
        self,
        key: Q::Key,
        span: Span,
        dep_node: DepNode,
    ) {
        // We may be concurrently trying both execute and force a query.
        // Ensure that only one of them runs the query.

        self.try_get_cached::<Q, _, _, _>(
            key,
            |_, _| {
                // Cache hit, do nothing
            },
            |key, lookup| {
                let job = match JobOwner::try_start(self, span, &key, lookup) {
                    TryGetJob::NotYetStarted(job) => job,
                    TryGetJob::Cycle(_) => return,
                    #[cfg(parallel_compiler)]
                    TryGetJob::JobCompleted(_) => return,
                };
                self.force_query_with_job::<Q>(key, job, dep_node);
            },
        );
    }
}

macro_rules! handle_cycle_error {
    ([][$tcx: expr, $error:expr]) => {{
        $tcx.report_cycle($error).emit();
        Value::from_cycle_error($tcx)
    }};
    ([fatal_cycle $($rest:tt)*][$tcx:expr, $error:expr]) => {{
        $tcx.report_cycle($error).emit();
        $tcx.sess.abort_if_errors();
        unreachable!()
    }};
    ([cycle_delay_bug $($rest:tt)*][$tcx:expr, $error:expr]) => {{
        $tcx.report_cycle($error).delay_as_bug();
        Value::from_cycle_error($tcx)
    }};
    ([$other:ident $(($($other_args:tt)*))* $(, $($modifiers:tt)*)*][$($args:tt)*]) => {
        handle_cycle_error!([$($($modifiers)*)*][$($args)*])
    };
}

macro_rules! is_anon {
    ([]) => {{
        false
    }};
    ([anon $($rest:tt)*]) => {{
        true
    }};
    ([$other:ident $(($($other_args:tt)*))* $(, $($modifiers:tt)*)*]) => {
        is_anon!([$($($modifiers)*)*])
    };
}

macro_rules! is_eval_always {
    ([]) => {{
        false
    }};
    ([eval_always $($rest:tt)*]) => {{
        true
    }};
    ([$other:ident $(($($other_args:tt)*))* $(, $($modifiers:tt)*)*]) => {
        is_eval_always!([$($($modifiers)*)*])
    };
}

macro_rules! query_storage {
    ([][$K:ty, $V:ty]) => {
        <<$K as Key>::CacheSelector as CacheSelector<$K, $V>>::Cache
    };
    ([storage($ty:ty) $($rest:tt)*][$K:ty, $V:ty]) => {
        $ty
    };
    ([$other:ident $(($($other_args:tt)*))* $(, $($modifiers:tt)*)*][$($args:tt)*]) => {
        query_storage!([$($($modifiers)*)*][$($args)*])
    };
}

macro_rules! hash_result {
    ([][$hcx:expr, $result:expr]) => {{
        dep_graph::hash_result($hcx, &$result)
    }};
    ([no_hash $($rest:tt)*][$hcx:expr, $result:expr]) => {{
        None
    }};
    ([$other:ident $(($($other_args:tt)*))* $(, $($modifiers:tt)*)*][$($args:tt)*]) => {
        hash_result!([$($($modifiers)*)*][$($args)*])
    };
}

macro_rules! define_queries {
    (<$tcx:tt> $($category:tt {
        $($(#[$attr:meta])* [$($modifiers:tt)*] fn $name:ident: $node:ident($K:ty) -> $V:ty,)*
    },)*) => {
        define_queries_inner! { <$tcx>
            $($( $(#[$attr])* category<$category> [$($modifiers)*] fn $name: $node($K) -> $V,)*)*
        }
    }
}

macro_rules! define_queries_inner {
    (<$tcx:tt>
     $($(#[$attr:meta])* category<$category:tt>
        [$($modifiers:tt)*] fn $name:ident: $node:ident($K:ty) -> $V:ty,)*) => {

        use std::mem;
        use crate::{
            rustc_data_structures::stable_hasher::HashStable,
            rustc_data_structures::stable_hasher::StableHasher,
            ich::StableHashingContext
        };
        use rustc_data_structures::profiling::ProfileCategory;

        define_queries_struct! {
            tcx: $tcx,
            input: ($(([$($modifiers)*] [$($attr)*] [$name]))*)
        }

        impl<$tcx> Queries<$tcx> {
            pub fn new(
                providers: IndexVec<CrateNum, Providers<$tcx>>,
                fallback_extern_providers: Providers<$tcx>,
                on_disk_cache: OnDiskCache<'tcx>,
            ) -> Self {
                Queries {
                    providers,
                    fallback_extern_providers: Box::new(fallback_extern_providers),
                    on_disk_cache,
                    $($name: Default::default()),*
                }
            }

            pub fn try_collect_active_jobs(
                &self
            ) -> Option<FxHashMap<QueryJobId, QueryJobInfo<'tcx>>> {
                let mut jobs = FxHashMap::default();

                $(
                    // We use try_lock_shards here since we are called from the
                    // deadlock handler, and this shouldn't be locked.
                    let shards = self.$name.shards.try_lock_shards()?;
                    let shards = shards.iter().enumerate();
                    jobs.extend(shards.flat_map(|(shard_id, shard)| {
                        shard.active.iter().filter_map(move |(k, v)| {
                        if let QueryResult::Started(ref job) = *v {
                                let id = QueryJobId {
                                    job: job.id,
                                    shard:  u16::try_from(shard_id).unwrap(),
                                    kind:
                                        <queries::$name<'tcx> as QueryAccessors<'tcx>>::dep_kind(),
                                };
                                let info = QueryInfo {
                                    span: job.span,
                                    query: queries::$name::query(k.clone())
                                };
                                Some((id, QueryJobInfo { info,  job: job.clone() }))
                        } else {
                            None
                        }
                        })
                    }));
                )*

                Some(jobs)
            }
        }

        #[allow(nonstandard_style)]
        #[derive(Clone, Debug)]
        pub enum Query<$tcx> {
            $($(#[$attr])* $name($K)),*
        }

        impl<$tcx> Query<$tcx> {
            pub fn name(&self) -> &'static str {
                match *self {
                    $(Query::$name(_) => stringify!($name),)*
                }
            }

            pub fn describe(&self, tcx: TyCtxt<'_>) -> Cow<'static, str> {
                let (r, name) = match *self {
                    $(Query::$name(key) => {
                        (queries::$name::describe(tcx, key), stringify!($name))
                    })*
                };
                if tcx.sess.verbose() {
                    format!("{} [{}]", r, name).into()
                } else {
                    r
                }
            }

            // FIXME(eddyb) Get more valid `Span`s on queries.
            pub fn default_span(&self, tcx: TyCtxt<$tcx>, span: Span) -> Span {
                if !span.is_dummy() {
                    return span;
                }
                // The `def_span` query is used to calculate `default_span`,
                // so exit to avoid infinite recursion.
                if let Query::def_span(..) = *self {
                    return span
                }
                match *self {
                    $(Query::$name(key) => key.default_span(tcx),)*
                }
            }
        }

        impl<'a, $tcx> HashStable<StableHashingContext<'a>> for Query<$tcx> {
            fn hash_stable(&self, hcx: &mut StableHashingContext<'a>, hasher: &mut StableHasher) {
                mem::discriminant(self).hash_stable(hcx, hasher);
                match *self {
                    $(Query::$name(key) => key.hash_stable(hcx, hasher),)*
                }
            }
        }

        pub mod queries {
            use std::marker::PhantomData;

            $(#[allow(nonstandard_style)]
            pub struct $name<$tcx> {
                data: PhantomData<&$tcx ()>
            })*
        }

        // This module and the functions in it exist only to provide a
        // predictable symbol name prefix for query providers. This is helpful
        // for analyzing queries in profilers.
        pub(super) mod __query_compute {
            $(#[inline(never)]
            pub fn $name<F: FnOnce() -> R, R>(f: F) -> R {
                f()
            })*
        }

        $(impl<$tcx> QueryConfig<$tcx> for queries::$name<$tcx> {
            type Key = $K;
            type Value = $V;
            const NAME: &'static str = stringify!($name);
            const CATEGORY: ProfileCategory = $category;
        }

        impl<$tcx> QueryAccessors<$tcx> for queries::$name<$tcx> {
            const ANON: bool = is_anon!([$($modifiers)*]);
            const EVAL_ALWAYS: bool = is_eval_always!([$($modifiers)*]);

            type Cache = query_storage!([$($modifiers)*][$K, $V]);

            #[inline(always)]
            fn query(key: Self::Key) -> Query<'tcx> {
                Query::$name(key)
            }

            #[inline(always)]
            fn query_state<'a>(tcx: TyCtxt<$tcx>) -> &'a QueryState<$tcx, Self> {
                &tcx.queries.$name
            }

            #[allow(unused)]
            #[inline(always)]
            fn to_dep_node(tcx: TyCtxt<$tcx>, key: &Self::Key) -> DepNode {
                DepConstructor::$node(tcx, *key)
            }

            #[inline(always)]
            fn dep_kind() -> dep_graph::DepKind {
                dep_graph::DepKind::$node
            }

            #[inline]
            fn compute(tcx: TyCtxt<'tcx>, key: Self::Key) -> Self::Value {
                __query_compute::$name(move || {
                    let provider = tcx.queries.providers.get(key.query_crate())
                        // HACK(eddyb) it's possible crates may be loaded after
                        // the query engine is created, and because crate loading
                        // is not yet integrated with the query engine, such crates
                        // would be missing appropriate entries in `providers`.
                        .unwrap_or(&tcx.queries.fallback_extern_providers)
                        .$name;
                    provider(tcx, key)
                })
            }

            fn hash_result(
                _hcx: &mut StableHashingContext<'_>,
                _result: &Self::Value
            ) -> Option<Fingerprint> {
                hash_result!([$($modifiers)*][_hcx, _result])
            }

            fn handle_cycle_error(
                tcx: TyCtxt<'tcx>,
                error: CycleError<'tcx>
            ) -> Self::Value {
                handle_cycle_error!([$($modifiers)*][tcx, error])
            }
        })*

        #[derive(Copy, Clone)]
        pub struct TyCtxtEnsure<'tcx> {
            pub tcx: TyCtxt<'tcx>,
        }

        impl TyCtxtEnsure<$tcx> {
            $($(#[$attr])*
            #[inline(always)]
            pub fn $name(self, key: $K) {
                self.tcx.ensure_query::<queries::$name<'_>>(key)
            })*
        }

        #[derive(Copy, Clone)]
        pub struct TyCtxtAt<'tcx> {
            pub tcx: TyCtxt<'tcx>,
            pub span: Span,
        }

        impl Deref for TyCtxtAt<'tcx> {
            type Target = TyCtxt<'tcx>;
            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                &self.tcx
            }
        }

        impl TyCtxt<$tcx> {
            /// Returns a transparent wrapper for `TyCtxt`, which ensures queries
            /// are executed instead of just returning their results.
            #[inline(always)]
            pub fn ensure(self) -> TyCtxtEnsure<$tcx> {
                TyCtxtEnsure {
                    tcx: self,
                }
            }

            /// Returns a transparent wrapper for `TyCtxt` which uses
            /// `span` as the location of queries performed through it.
            #[inline(always)]
            pub fn at(self, span: Span) -> TyCtxtAt<$tcx> {
                TyCtxtAt {
                    tcx: self,
                    span
                }
            }

            $($(#[$attr])*
            #[inline(always)]
            pub fn $name(self, key: $K) -> $V {
                self.at(DUMMY_SP).$name(key)
            })*

            /// All self-profiling events generated by the query engine use
            /// virtual `StringId`s for their `event_id`. This method makes all
            /// those virtual `StringId`s point to actual strings.
            ///
            /// If we are recording only summary data, the ids will point to
            /// just the query names. If we are recording query keys too, we
            /// allocate the corresponding strings here.
            pub fn alloc_self_profile_query_strings(self) {
                use crate::ty::query::profiling_support::{
                    alloc_self_profile_query_strings_for_query_cache,
                    QueryKeyStringCache,
                };

                if !self.prof.enabled() {
                    return;
                }

                let mut string_cache = QueryKeyStringCache::new();

                $({
                    alloc_self_profile_query_strings_for_query_cache(
                        self,
                        stringify!($name),
                        &self.queries.$name,
                        &mut string_cache,
                    );
                })*
            }
        }

        impl TyCtxtAt<$tcx> {
            $($(#[$attr])*
            #[inline(always)]
            pub fn $name(self, key: $K) -> $V {
                self.tcx.get_query::<queries::$name<'_>>(self.span, key)
            })*
        }

        define_provider_struct! {
            tcx: $tcx,
            input: ($(([$($modifiers)*] [$name] [$K] [$V]))*)
        }

        impl<$tcx> Copy for Providers<$tcx> {}
        impl<$tcx> Clone for Providers<$tcx> {
            fn clone(&self) -> Self { *self }
        }
    }
}

macro_rules! define_queries_struct {
    (tcx: $tcx:tt,
     input: ($(([$($modifiers:tt)*] [$($attr:tt)*] [$name:ident]))*)) => {
        pub struct Queries<$tcx> {
            /// This provides access to the incrimental comilation on-disk cache for query results.
            /// Do not access this directly. It is only meant to be used by
            /// `DepGraph::try_mark_green()` and the query infrastructure.
            pub(crate) on_disk_cache: OnDiskCache<'tcx>,

            providers: IndexVec<CrateNum, Providers<$tcx>>,
            fallback_extern_providers: Box<Providers<$tcx>>,

            $($(#[$attr])*  $name: QueryState<$tcx, queries::$name<$tcx>>,)*
        }
    };
}

macro_rules! define_provider_struct {
    (tcx: $tcx:tt,
     input: ($(([$($modifiers:tt)*] [$name:ident] [$K:ty] [$R:ty]))*)) => {
        pub struct Providers<$tcx> {
            $(pub $name: fn(TyCtxt<$tcx>, $K) -> $R,)*
        }

        impl<$tcx> Default for Providers<$tcx> {
            fn default() -> Self {
                $(fn $name<$tcx>(_: TyCtxt<$tcx>, key: $K) -> $R {
                    bug!("`tcx.{}({:?})` unsupported by its crate",
                         stringify!($name), key);
                })*
                Providers { $($name),* }
            }
        }
    };
}
