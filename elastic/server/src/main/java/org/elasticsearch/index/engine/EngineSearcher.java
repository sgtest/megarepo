/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.index.engine;

import org.apache.logging.log4j.Logger;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.ReferenceManager;
import org.apache.lucene.search.SearcherManager;
import org.apache.lucene.store.AlreadyClosedException;
import org.elasticsearch.index.store.Store;

import java.io.Closeable;
import java.io.IOException;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Searcher for an Engine
 */
final class EngineSearcher extends Engine.Searcher {
    private final AtomicBoolean released = new AtomicBoolean(false);
    private final Store store;
    private final Logger logger;
    private final ReferenceManager<IndexSearcher> referenceManager;

    EngineSearcher(String source, ReferenceManager<IndexSearcher> searcherReferenceManager, Store store, Logger logger) throws IOException {
        super(source, searcherReferenceManager.acquire());
        this.store = store;
        this.logger = logger;
        this.referenceManager = searcherReferenceManager;
    }

    @Override
    public void close() {
        if (!released.compareAndSet(false, true)) {
                /* In general, searchers should never be released twice or this would break reference counting. There is one rare case
                 * when it might happen though: when the request and the Reaper thread would both try to release it in a very short amount
                 * of time, this is why we only log a warning instead of throwing an exception.
                 */
            logger.warn("Searcher was released twice", new IllegalStateException("Double release"));
            return;
        }
        try {
            referenceManager.release(searcher());
        } catch (IOException e) {
            throw new IllegalStateException("Cannot close", e);
        } catch (AlreadyClosedException e) {
            // This means there's a bug somewhere: don't suppress it
            throw new AssertionError(e);
        } finally {
            store.decRef();
        }
    }
}
