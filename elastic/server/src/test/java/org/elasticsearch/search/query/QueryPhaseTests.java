/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.query;

import org.apache.lucene.analysis.standard.StandardAnalyzer;
import org.apache.lucene.document.Document;
import org.apache.lucene.document.Field.Store;
import org.apache.lucene.document.LatLonDocValuesField;
import org.apache.lucene.document.LatLonPoint;
import org.apache.lucene.document.LongPoint;
import org.apache.lucene.document.NumericDocValuesField;
import org.apache.lucene.document.SortedSetDocValuesField;
import org.apache.lucene.document.StringField;
import org.apache.lucene.document.TextField;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.IndexReader;
import org.apache.lucene.index.IndexWriter;
import org.apache.lucene.index.IndexWriterConfig;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.index.NoMergePolicy;
import org.apache.lucene.index.Term;
import org.apache.lucene.queries.spans.SpanNearQuery;
import org.apache.lucene.queries.spans.SpanTermQuery;
import org.apache.lucene.search.BooleanClause.Occur;
import org.apache.lucene.search.BooleanQuery;
import org.apache.lucene.search.Collector;
import org.apache.lucene.search.ConstantScoreQuery;
import org.apache.lucene.search.FieldComparator;
import org.apache.lucene.search.FieldDoc;
import org.apache.lucene.search.FieldExistsQuery;
import org.apache.lucene.search.FilterCollector;
import org.apache.lucene.search.FilterLeafCollector;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.LeafCollector;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.search.MatchNoDocsQuery;
import org.apache.lucene.search.MultiTermQuery;
import org.apache.lucene.search.PrefixQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.ScoreDoc;
import org.apache.lucene.search.Sort;
import org.apache.lucene.search.SortField;
import org.apache.lucene.search.TermQuery;
import org.apache.lucene.search.TopDocs;
import org.apache.lucene.search.TotalHitCountCollector;
import org.apache.lucene.search.TotalHits;
import org.apache.lucene.search.Weight;
import org.apache.lucene.search.join.BitSetProducer;
import org.apache.lucene.search.join.ScoreMode;
import org.apache.lucene.store.Directory;
import org.apache.lucene.tests.index.RandomIndexWriter;
import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.FixedBitSet;
import org.elasticsearch.action.search.SearchShardTask;
import org.elasticsearch.index.mapper.DateFieldMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.NumberFieldMapper;
import org.elasticsearch.index.query.ParsedQuery;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.index.search.ESToParentBlockJoinQuery;
import org.elasticsearch.index.shard.IndexShard;
import org.elasticsearch.index.shard.IndexShardTestCase;
import org.elasticsearch.lucene.queries.MinDocQuery;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.internal.ContextIndexSearcher;
import org.elasticsearch.search.internal.ScrollContext;
import org.elasticsearch.search.internal.SearchContext;
import org.elasticsearch.search.sort.SortAndFormats;
import org.elasticsearch.tasks.TaskCancelHelper;
import org.elasticsearch.tasks.TaskCancelledException;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.test.TestSearchContext;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.function.IntUnaryOperator;

import static org.elasticsearch.search.query.TopDocsCollectorContext.hasInfMaxScore;
import static org.hamcrest.Matchers.anyOf;
import static org.hamcrest.Matchers.arrayWithSize;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.lessThan;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class QueryPhaseTests extends IndexShardTestCase {

    private IndexShard indexShard;

    @Override
    public void setUp() throws Exception {
        super.setUp();
        indexShard = newShard(true);
    }

    @Override
    public void tearDown() throws Exception {
        super.tearDown();
        closeShards(indexShard);
    }

    private void countTestCase(Query query, IndexReader reader, boolean shouldCollectSearch, boolean shouldCollectCount) throws Exception {
        ContextIndexSearcher searcher = shouldCollectSearch ? newContextSearcher(reader) : newEarlyTerminationContextSearcher(reader, 0);
        TestSearchContext context = new TestSearchContext(null, indexShard, searcher);
        context.parsedQuery(new ParsedQuery(query));
        context.setSize(0);
        context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
        final boolean rescore = QueryPhase.executeInternal(context);
        assertFalse(rescore);

        ContextIndexSearcher countSearcher = shouldCollectCount
            ? newContextSearcher(reader)
            : newEarlyTerminationContextSearcher(reader, 0);
        assertEquals(countSearcher.count(query), context.queryResult().topDocs().topDocs.totalHits.value);
    }

    private void countTestCase(boolean withDeletions) throws Exception {
        Directory dir = newDirectory();
        IndexWriterConfig iwc = newIndexWriterConfig().setMergePolicy(NoMergePolicy.INSTANCE);
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        final int numDocs = scaledRandomIntBetween(100, 200);
        for (int i = 0; i < numDocs; ++i) {
            Document doc = new Document();
            if (randomBoolean()) {
                doc.add(new StringField("foo", "bar", Store.NO));
                doc.add(new SortedSetDocValuesField("foo", new BytesRef("bar")));
                doc.add(new SortedSetDocValuesField("docValuesOnlyField", new BytesRef("bar")));
                doc.add(new LatLonDocValuesField("latLonDVField", 1.0, 1.0));
                doc.add(new LatLonPoint("latLonDVField", 1.0, 1.0));
            }
            if (randomBoolean()) {
                doc.add(new StringField("foo", "baz", Store.NO));
                doc.add(new SortedSetDocValuesField("foo", new BytesRef("baz")));
            }
            if (withDeletions && (rarely() || i == 0)) {
                doc.add(new StringField("delete", "yes", Store.NO));
            }
            w.addDocument(doc);
        }
        if (withDeletions) {
            w.deleteDocuments(new Term("delete", "yes"));
        }
        final IndexReader reader = w.getReader();
        Query matchAll = new MatchAllDocsQuery();
        Query matchAllCsq = new ConstantScoreQuery(matchAll);
        Query tq = new TermQuery(new Term("foo", "bar"));
        Query tCsq = new ConstantScoreQuery(tq);
        Query feq = new FieldExistsQuery("foo");
        Query feq_points = new FieldExistsQuery("latLonDVField");
        Query feqCsq = new ConstantScoreQuery(feq);
        // field with doc-values but not indexed will need to collect
        Query dvOnlyfeq = new FieldExistsQuery("docValuesOnlyField");
        BooleanQuery bq = new BooleanQuery.Builder().add(matchAll, Occur.SHOULD).add(tq, Occur.MUST).build();

        countTestCase(matchAll, reader, false, false);
        countTestCase(matchAllCsq, reader, false, false);
        countTestCase(tq, reader, withDeletions, withDeletions);
        countTestCase(tCsq, reader, withDeletions, withDeletions);
        countTestCase(feq, reader, withDeletions, true);
        countTestCase(feq_points, reader, withDeletions, true);
        countTestCase(feqCsq, reader, withDeletions, true);
        countTestCase(dvOnlyfeq, reader, true, true);
        countTestCase(bq, reader, true, true);
        reader.close();
        w.close();
        dir.close();
    }

    public void testCountWithoutDeletions() throws Exception {
        countTestCase(false);
    }

    public void testCountWithDeletions() throws Exception {
        countTestCase(true);
    }

    public void testPostFilterDisablesCountOptimization() throws Exception {
        Directory dir = newDirectory();
        final Sort sort = new Sort(new SortField("rank", SortField.Type.INT));
        IndexWriterConfig iwc = newIndexWriterConfig().setIndexSort(sort);
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        Document doc = new Document();
        w.addDocument(doc);
        w.close();

        IndexReader reader = DirectoryReader.open(dir);
        TestSearchContext context = new TestSearchContext(null, indexShard, newEarlyTerminationContextSearcher(reader, 0));
        context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
        context.parsedQuery(new ParsedQuery(new MatchAllDocsQuery()));

        QueryPhase.executeInternal(context);
        assertEquals(1, context.queryResult().topDocs().topDocs.totalHits.value);

        context.setSearcher(newContextSearcher(reader));
        context.parsedPostFilter(new ParsedQuery(new MatchNoDocsQuery()));
        QueryPhase.executeInternal(context);
        assertEquals(0, context.queryResult().topDocs().topDocs.totalHits.value);
        reader.close();
        dir.close();
    }

    public void testTerminateAfterWithFilter() throws Exception {
        Directory dir = newDirectory();
        final Sort sort = new Sort(new SortField("rank", SortField.Type.INT));
        IndexWriterConfig iwc = newIndexWriterConfig().setIndexSort(sort);
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        Document doc = new Document();
        for (int i = 0; i < 10; i++) {
            doc.add(new StringField("foo", Integer.toString(i), Store.NO));
        }
        w.addDocument(doc);
        w.close();

        IndexReader reader = DirectoryReader.open(dir);
        TestSearchContext context = new TestSearchContext(null, indexShard, newContextSearcher(reader));
        context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
        context.parsedQuery(new ParsedQuery(new MatchAllDocsQuery()));
        context.terminateAfter(1);
        context.setSize(10);
        for (int i = 0; i < 10; i++) {
            context.parsedPostFilter(new ParsedQuery(new TermQuery(new Term("foo", Integer.toString(i)))));
            QueryPhase.executeInternal(context);
            assertEquals(1, context.queryResult().topDocs().topDocs.totalHits.value);
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(1));
        }
        reader.close();
        dir.close();
    }

    public void testMinScoreDisablesCountOptimization() throws Exception {
        Directory dir = newDirectory();
        final Sort sort = new Sort(new SortField("rank", SortField.Type.INT));
        IndexWriterConfig iwc = newIndexWriterConfig().setIndexSort(sort);
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        Document doc = new Document();
        w.addDocument(doc);
        w.close();

        IndexReader reader = DirectoryReader.open(dir);
        TestSearchContext context = new TestSearchContext(null, indexShard, newEarlyTerminationContextSearcher(reader, 0));
        context.parsedQuery(new ParsedQuery(new MatchAllDocsQuery()));
        context.setSize(0);
        context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
        QueryPhase.executeInternal(context);
        assertEquals(1, context.queryResult().topDocs().topDocs.totalHits.value);

        context.minimumScore(100);
        QueryPhase.executeInternal(context);
        assertEquals(0, context.queryResult().topDocs().topDocs.totalHits.value);
        reader.close();
        dir.close();
    }

    public void testQueryCapturesThreadPoolStats() throws Exception {
        Directory dir = newDirectory();
        IndexWriterConfig iwc = newIndexWriterConfig();
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        final int numDocs = scaledRandomIntBetween(100, 200);
        for (int i = 0; i < numDocs; ++i) {
            w.addDocument(new Document());
        }
        w.close();
        IndexReader reader = DirectoryReader.open(dir);
        TestSearchContext context = new TestSearchContext(null, indexShard, newContextSearcher(reader));
        context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
        context.parsedQuery(new ParsedQuery(new MatchAllDocsQuery()));

        QueryPhase.executeInternal(context);
        QuerySearchResult results = context.queryResult();
        assertThat(results.serviceTimeEWMA(), greaterThanOrEqualTo(0L));
        assertThat(results.nodeQueueSize(), greaterThanOrEqualTo(0));
        reader.close();
        dir.close();
    }

    public void testInOrderScrollOptimization() throws Exception {
        Directory dir = newDirectory();
        final Sort sort = new Sort(new SortField("rank", SortField.Type.INT));
        IndexWriterConfig iwc = newIndexWriterConfig().setIndexSort(sort);
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        final int numDocs = scaledRandomIntBetween(100, 200);
        for (int i = 0; i < numDocs; ++i) {
            w.addDocument(new Document());
        }
        w.close();
        IndexReader reader = DirectoryReader.open(dir);
        ScrollContext scrollContext = new ScrollContext();
        TestSearchContext context = new TestSearchContext(null, indexShard, newContextSearcher(reader), scrollContext);
        context.parsedQuery(new ParsedQuery(new MatchAllDocsQuery()));
        context.sort(new SortAndFormats(sort, new DocValueFormat[] { DocValueFormat.RAW }));
        scrollContext.lastEmittedDoc = null;
        scrollContext.maxScore = Float.NaN;
        scrollContext.totalHits = null;
        context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
        int size = randomIntBetween(2, 5);
        context.setSize(size);

        QueryPhase.executeInternal(context);
        assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) numDocs));
        assertNull(context.queryResult().terminatedEarly());
        assertThat(context.terminateAfter(), equalTo(0));
        assertThat(context.queryResult().getTotalHits().value, equalTo((long) numDocs));

        context.setSearcher(newEarlyTerminationContextSearcher(reader, size));
        QueryPhase.executeInternal(context);
        assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) numDocs));
        assertThat(context.queryResult().getTotalHits().value, equalTo((long) numDocs));
        assertThat(context.queryResult().topDocs().topDocs.scoreDocs[0].doc, greaterThanOrEqualTo(size));
        reader.close();
        dir.close();
    }

    public void testTerminateAfterEarlyTermination() throws Exception {
        Directory dir = newDirectory();
        IndexWriterConfig iwc = newIndexWriterConfig();
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        final int numDocs = scaledRandomIntBetween(100, 200);
        for (int i = 0; i < numDocs; ++i) {
            Document doc = new Document();
            if (randomBoolean()) {
                doc.add(new StringField("foo", "bar", Store.NO));
            }
            if (randomBoolean()) {
                doc.add(new StringField("foo", "baz", Store.NO));
            }
            doc.add(new NumericDocValuesField("rank", numDocs - i));
            w.addDocument(doc);
        }
        w.close();
        final IndexReader reader = DirectoryReader.open(dir);
        // TotalHitCountCollector can shortcut count until the terminate_after
        IntUnaryOperator countDocUpTo = terminateAfter -> {
            int total = 0;
            for (LeafReaderContext leaf : reader.leaves()) {
                total += leaf.reader().numDocs();
                if (total >= terminateAfter) {
                    break;
                }
            }
            return total;
        };
        TestSearchContext context = new TestSearchContext(null, indexShard, newContextSearcher(reader));
        context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
        context.parsedQuery(new ParsedQuery(new MatchAllDocsQuery()));

        context.terminateAfter(numDocs);
        {
            context.setSize(10);
            TotalHitCountCollector collector = new TotalHitCountCollector();
            context.queryCollectors().put(TotalHitCountCollector.class, collector);
            QueryPhase.executeInternal(context);
            assertFalse(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) numDocs));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(10));
            assertThat(collector.getTotalHits(), equalTo(numDocs));
        }

        context.terminateAfter(1);
        {
            context.setSize(1);
            QueryPhase.executeInternal(context);
            assertTrue(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo(1L));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(1));

            context.setSize(0);
            QueryPhase.executeInternal(context);
            assertTrue(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) countDocUpTo.applyAsInt(1)));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(0));
        }

        {
            context.setSize(1);
            QueryPhase.executeInternal(context);
            assertTrue(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo(1L));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(1));
        }
        {
            context.setSize(1);
            BooleanQuery bq = new BooleanQuery.Builder().add(new TermQuery(new Term("foo", "bar")), Occur.SHOULD)
                .add(new TermQuery(new Term("foo", "baz")), Occur.SHOULD)
                .build();
            context.parsedQuery(new ParsedQuery(bq));
            QueryPhase.executeInternal(context);
            assertTrue(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo(1L));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(1));

            context.setSize(0);
            context.parsedQuery(new ParsedQuery(bq));
            QueryPhase.executeInternal(context);
            assertTrue(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(0));
            context.parsedQuery(new ParsedQuery(new MatchAllDocsQuery()));
        }
        {
            context.setSize(1);
            TotalHitCountCollector collector = new TotalHitCountCollector();
            context.queryCollectors().put(TotalHitCountCollector.class, collector);
            QueryPhase.executeInternal(context);
            assertTrue(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo(1L));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(1));
            // TotalHitCountCollector counts num docs in the first leaf
            assertThat(collector.getTotalHits(), equalTo(reader.leaves().get(0).reader().numDocs()));
            context.queryCollectors().clear();
        }
        {
            context.setSize(0);
            TotalHitCountCollector collector = new TotalHitCountCollector();
            context.queryCollectors().put(TotalHitCountCollector.class, collector);
            QueryPhase.executeInternal(context);
            assertTrue(context.queryResult().terminatedEarly());
            // TotalHitCountCollector counts num docs in the first leaf
            int numDocsInFirstLeaf = reader.leaves().get(0).reader().numDocs();
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) numDocsInFirstLeaf));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(0));
            assertThat(collector.getTotalHits(), equalTo(numDocsInFirstLeaf));
        }

        // tests with trackTotalHits and terminateAfter
        context.terminateAfter(10);
        context.setSize(0);
        for (int trackTotalHits : new int[] { -1, 3, 76, 100 }) {
            context.trackTotalHitsUpTo(trackTotalHits);
            TotalHitCountCollector collector = new TotalHitCountCollector();
            context.queryCollectors().put(TotalHitCountCollector.class, collector);
            QueryPhase.executeInternal(context);
            assertTrue(context.queryResult().terminatedEarly());
            if (trackTotalHits == -1) {
                assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo(0L));
            } else {
                assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) countDocUpTo.applyAsInt(10)));
            }
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(0));
            assertThat(collector.getTotalHits(), equalTo(countDocUpTo.applyAsInt(10)));
        }

        context.terminateAfter(7);
        context.setSize(10);
        for (int trackTotalHits : new int[] { -1, 3, 75, 100 }) {
            context.trackTotalHitsUpTo(trackTotalHits);
            EarlyTerminatingCollector collector = new EarlyTerminatingCollector(new TotalHitCountCollector(), 1, false);
            context.queryCollectors().put(EarlyTerminatingCollector.class, collector);
            QueryPhase.executeInternal(context);
            assertTrue(context.queryResult().terminatedEarly());
            if (trackTotalHits == -1) {
                assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo(0L));
            } else {
                assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo(7L));
            }
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(7));
        }
        reader.close();
        dir.close();
    }

    public void testIndexSortingEarlyTermination() throws Exception {
        Directory dir = newDirectory();
        final Sort sort = new Sort(new SortField("rank", SortField.Type.INT));
        IndexWriterConfig iwc = newIndexWriterConfig().setIndexSort(sort);
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        final int numDocs = scaledRandomIntBetween(100, 200);
        for (int i = 0; i < numDocs; ++i) {
            Document doc = new Document();
            if (randomBoolean()) {
                doc.add(new StringField("foo", "bar", Store.NO));
            }
            if (randomBoolean()) {
                doc.add(new StringField("foo", "baz", Store.NO));
            }
            doc.add(new NumericDocValuesField("rank", numDocs - i));
            w.addDocument(doc);
        }
        w.close();

        final IndexReader reader = DirectoryReader.open(dir);
        TestSearchContext context = new TestSearchContext(null, indexShard, newContextSearcher(reader));
        context.parsedQuery(new ParsedQuery(new MatchAllDocsQuery()));
        context.setSize(1);
        context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
        context.sort(new SortAndFormats(sort, new DocValueFormat[] { DocValueFormat.RAW }));

        QueryPhase.executeInternal(context);
        assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) numDocs));
        assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(1));
        assertThat(context.queryResult().topDocs().topDocs.scoreDocs[0], instanceOf(FieldDoc.class));
        FieldDoc fieldDoc = (FieldDoc) context.queryResult().topDocs().topDocs.scoreDocs[0];
        assertThat(fieldDoc.fields[0], equalTo(1));

        {
            context.parsedPostFilter(new ParsedQuery(new MinDocQuery(1)));
            QueryPhase.executeInternal(context);
            assertNull(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo(numDocs - 1L));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(1));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs[0], instanceOf(FieldDoc.class));
            assertThat(fieldDoc.fields[0], anyOf(equalTo(1), equalTo(2)));
            context.parsedPostFilter(null);

            final TotalHitCountCollector totalHitCountCollector = new TotalHitCountCollector();
            context.queryCollectors().put(TotalHitCountCollector.class, totalHitCountCollector);
            QueryPhase.executeInternal(context);
            assertNull(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) numDocs));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(1));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs[0], instanceOf(FieldDoc.class));
            assertThat(fieldDoc.fields[0], anyOf(equalTo(1), equalTo(2)));
            assertThat(totalHitCountCollector.getTotalHits(), equalTo(numDocs));
            context.queryCollectors().clear();
        }

        {
            context.setSearcher(newEarlyTerminationContextSearcher(reader, 1));
            context.trackTotalHitsUpTo(SearchContext.TRACK_TOTAL_HITS_DISABLED);
            QueryPhase.executeInternal(context);
            assertNull(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(1));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs[0], instanceOf(FieldDoc.class));
            assertThat(fieldDoc.fields[0], anyOf(equalTo(1), equalTo(2)));

            QueryPhase.executeInternal(context);
            assertNull(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(1));
            assertThat(context.queryResult().topDocs().topDocs.scoreDocs[0], instanceOf(FieldDoc.class));
            assertThat(fieldDoc.fields[0], anyOf(equalTo(1), equalTo(2)));
        }
        reader.close();
        dir.close();
    }

    public void testIndexSortScrollOptimization() throws Exception {
        Directory dir = newDirectory();
        final Sort indexSort = new Sort(new SortField("rank", SortField.Type.INT), new SortField("tiebreaker", SortField.Type.INT));
        IndexWriterConfig iwc = newIndexWriterConfig().setIndexSort(indexSort);
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        final int numDocs = scaledRandomIntBetween(100, 200);
        for (int i = 0; i < numDocs; ++i) {
            Document doc = new Document();
            doc.add(new NumericDocValuesField("rank", random().nextInt()));
            doc.add(new NumericDocValuesField("tiebreaker", i));
            w.addDocument(doc);
        }
        if (randomBoolean()) {
            w.forceMerge(randomIntBetween(1, 10));
        }
        w.close();

        final IndexReader reader = DirectoryReader.open(dir);
        List<SortAndFormats> searchSortAndFormats = new ArrayList<>();
        searchSortAndFormats.add(new SortAndFormats(indexSort, new DocValueFormat[] { DocValueFormat.RAW, DocValueFormat.RAW }));
        // search sort is a prefix of the index sort
        searchSortAndFormats.add(new SortAndFormats(new Sort(indexSort.getSort()[0]), new DocValueFormat[] { DocValueFormat.RAW }));
        for (SortAndFormats searchSortAndFormat : searchSortAndFormats) {
            ScrollContext scrollContext = new ScrollContext();
            TestSearchContext context = new TestSearchContext(null, indexShard, newContextSearcher(reader), scrollContext);
            context.parsedQuery(new ParsedQuery(new MatchAllDocsQuery()));
            scrollContext.lastEmittedDoc = null;
            scrollContext.maxScore = Float.NaN;
            scrollContext.totalHits = null;
            context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
            context.setSize(10);
            context.sort(searchSortAndFormat);

            QueryPhase.executeInternal(context);
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) numDocs));
            assertNull(context.queryResult().terminatedEarly());
            assertThat(context.terminateAfter(), equalTo(0));
            assertThat(context.queryResult().getTotalHits().value, equalTo((long) numDocs));
            int sizeMinus1 = context.queryResult().topDocs().topDocs.scoreDocs.length - 1;
            FieldDoc lastDoc = (FieldDoc) context.queryResult().topDocs().topDocs.scoreDocs[sizeMinus1];

            context.setSearcher(newEarlyTerminationContextSearcher(reader, 10));
            QueryPhase.executeInternal(context);
            assertNull(context.queryResult().terminatedEarly());
            assertThat(context.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) numDocs));
            assertThat(context.terminateAfter(), equalTo(0));
            assertThat(context.queryResult().getTotalHits().value, equalTo((long) numDocs));
            FieldDoc firstDoc = (FieldDoc) context.queryResult().topDocs().topDocs.scoreDocs[0];
            for (int i = 0; i < searchSortAndFormat.sort.getSort().length; i++) {
                @SuppressWarnings("unchecked")
                FieldComparator<Object> comparator = (FieldComparator<Object>) searchSortAndFormat.sort.getSort()[i].getComparator(
                    1,
                    i == 0
                );
                int cmp = comparator.compareValues(firstDoc.fields[i], lastDoc.fields[i]);
                if (cmp == 0) {
                    continue;
                }
                assertThat(cmp, equalTo(1));
                break;
            }
        }
        reader.close();
        dir.close();
    }

    public void testDisableTopScoreCollection() throws Exception {
        Directory dir = newDirectory();
        IndexWriterConfig iwc = newIndexWriterConfig(new StandardAnalyzer());
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        Document doc = new Document();
        for (int i = 0; i < 10; i++) {
            doc.clear();
            if (i % 2 == 0) {
                doc.add(new TextField("title", "foo bar", Store.NO));
            } else {
                doc.add(new TextField("title", "foo", Store.NO));
            }
            w.addDocument(doc);
        }
        w.close();

        IndexReader reader = DirectoryReader.open(dir);
        TestSearchContext context = new TestSearchContext(null, indexShard, newContextSearcher(reader));
        context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
        Query q = new SpanNearQuery.Builder("title", true).addClause(new SpanTermQuery(new Term("title", "foo")))
            .addClause(new SpanTermQuery(new Term("title", "bar")))
            .build();

        context.parsedQuery(new ParsedQuery(q));
        context.setSize(3);
        context.trackTotalHitsUpTo(3);
        TopDocsCollectorContext topDocsContext = TopDocsCollectorContext.createTopDocsCollectorContext(context);
        assertEquals(topDocsContext.create(null).scoreMode(), org.apache.lucene.search.ScoreMode.COMPLETE);
        QueryPhase.executeInternal(context);
        assertEquals(5, context.queryResult().topDocs().topDocs.totalHits.value);
        assertEquals(context.queryResult().topDocs().topDocs.totalHits.relation, TotalHits.Relation.EQUAL_TO);
        assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(3));

        context.sort(new SortAndFormats(new Sort(new SortField("other", SortField.Type.INT)), new DocValueFormat[] { DocValueFormat.RAW }));
        topDocsContext = TopDocsCollectorContext.createTopDocsCollectorContext(context);
        assertEquals(topDocsContext.create(null).scoreMode(), org.apache.lucene.search.ScoreMode.TOP_DOCS);
        QueryPhase.executeInternal(context);
        assertEquals(5, context.queryResult().topDocs().topDocs.totalHits.value);
        assertThat(context.queryResult().topDocs().topDocs.scoreDocs.length, equalTo(3));
        assertEquals(context.queryResult().topDocs().topDocs.totalHits.relation, TotalHits.Relation.GREATER_THAN_OR_EQUAL_TO);

        reader.close();
        dir.close();
    }

    public void testNumericSortOptimization() throws Exception {
        final String fieldNameLong = "long-field";
        final String fieldNameDate = "date-field";
        MappedFieldType fieldTypeLong = new NumberFieldMapper.NumberFieldType(fieldNameLong, NumberFieldMapper.NumberType.LONG);
        MappedFieldType fieldTypeDate = new DateFieldMapper.DateFieldType(fieldNameDate);
        SearchExecutionContext searchExecutionContext = mock(SearchExecutionContext.class);
        when(searchExecutionContext.getFieldType(fieldNameLong)).thenReturn(fieldTypeLong);
        when(searchExecutionContext.getFieldType(fieldNameDate)).thenReturn(fieldTypeDate);
        // enough docs to have a tree with several leaf nodes
        final int numDocs = atLeast(3500 * 2);
        Directory dir = newDirectory();
        IndexWriter writer = new IndexWriter(dir, new IndexWriterConfig(null));
        long startLongValue = randomLongBetween(-10000000L, 10000000L);
        long longValue = startLongValue;
        long dateValue = randomLongBetween(0, 3000000000000L);

        for (int i = 1; i <= numDocs; ++i) {
            Document doc = new Document();
            doc.add(new LongPoint(fieldNameLong, longValue));
            doc.add(new NumericDocValuesField(fieldNameLong, longValue));
            doc.add(new LongPoint(fieldNameDate, dateValue));
            doc.add(new NumericDocValuesField(fieldNameDate, dateValue));
            writer.addDocument(doc);
            longValue++;
            dateValue++;
            if (i % 3500 == 0) writer.flush();
        }
        writer.close();

        final IndexReader reader = DirectoryReader.open(dir);

        final SortField sortFieldLong = new SortField(fieldNameLong, SortField.Type.LONG);
        final SortField sortFieldDate = new SortField(fieldNameDate, SortField.Type.LONG);
        sortFieldLong.setMissingValue(Long.MAX_VALUE);
        sortFieldDate.setMissingValue(Long.MAX_VALUE);
        final Sort sortLong = new Sort(sortFieldLong);
        final Sort sortDate = new Sort(sortFieldDate);
        final Sort sortLongDate = new Sort(sortFieldLong, sortFieldDate);
        final Sort sortDateLong = new Sort(sortFieldDate, sortFieldLong);
        final DocValueFormat dvFormatDate = fieldTypeDate.docValueFormat(null, null);
        final SortAndFormats formatsLong = new SortAndFormats(sortLong, new DocValueFormat[] { DocValueFormat.RAW });
        final SortAndFormats formatsDate = new SortAndFormats(sortDate, new DocValueFormat[] { dvFormatDate });
        final SortAndFormats formatsLongDate = new SortAndFormats(sortLongDate, new DocValueFormat[] { DocValueFormat.RAW, dvFormatDate });
        final SortAndFormats formatsDateLong = new SortAndFormats(sortDateLong, new DocValueFormat[] { dvFormatDate, DocValueFormat.RAW });

        Query q = LongPoint.newRangeQuery(fieldNameLong, startLongValue, startLongValue + numDocs);
        final ParsedQuery query = new ParsedQuery(q);
        final SearchShardTask task = new SearchShardTask(123L, "", "", "", null, Collections.emptyMap());

        // 1. Test sort optimization on long field
        {
            TestSearchContext searchContext = new TestSearchContext(searchExecutionContext, indexShard, newContextSearcher(reader));
            searchContext.sort(formatsLong);
            searchContext.parsedQuery(query);
            searchContext.setTask(task);
            searchContext.trackTotalHitsUpTo(10);
            searchContext.setSize(10);
            QueryPhase.executeInternal(searchContext);
            assertTrue(searchContext.sort().sort.getSort()[0].getOptimizeSortWithPoints());
            assertSortResults(searchContext.queryResult().topDocs().topDocs, numDocs, false);
        }

        // 2. Test sort optimization on long field with after
        {
            TestSearchContext searchContext = new TestSearchContext(searchExecutionContext, indexShard, newContextSearcher(reader));
            int afterDoc = (int) randomLongBetween(0, 30);
            long afterValue = startLongValue + afterDoc;
            FieldDoc after = new FieldDoc(afterDoc, Float.NaN, new Long[] { afterValue });
            searchContext.searchAfter(after);
            searchContext.sort(formatsLong);
            searchContext.parsedQuery(query);
            searchContext.setTask(task);
            searchContext.trackTotalHitsUpTo(10);
            searchContext.setSize(10);
            QueryPhase.executeInternal(searchContext);
            assertTrue(searchContext.sort().sort.getSort()[0].getOptimizeSortWithPoints());
            final TopDocs topDocs = searchContext.queryResult().topDocs().topDocs;
            long firstResult = (long) ((FieldDoc) topDocs.scoreDocs[0]).fields[0];
            assertThat(firstResult, greaterThan(afterValue));
            assertSortResults(topDocs, numDocs, false);
        }

        // 3. Test sort optimization on long field + date field
        {
            TestSearchContext searchContext = new TestSearchContext(searchExecutionContext, indexShard, newContextSearcher(reader));
            searchContext.sort(formatsLongDate);
            searchContext.parsedQuery(query);
            searchContext.setTask(task);
            searchContext.trackTotalHitsUpTo(10);
            searchContext.setSize(10);
            QueryPhase.executeInternal(searchContext);
            assertTrue(searchContext.sort().sort.getSort()[0].getOptimizeSortWithPoints());
            assertSortResults(searchContext.queryResult().topDocs().topDocs, numDocs, true);
        }

        // 4. Test sort optimization on date field
        {
            TestSearchContext searchContext = new TestSearchContext(searchExecutionContext, indexShard, newContextSearcher(reader));
            searchContext.sort(formatsDate);
            searchContext.parsedQuery(query);
            searchContext.setTask(task);
            searchContext.trackTotalHitsUpTo(10);
            searchContext.setSize(10);
            QueryPhase.executeInternal(searchContext);
            assertTrue(searchContext.sort().sort.getSort()[0].getOptimizeSortWithPoints());
            assertSortResults(searchContext.queryResult().topDocs().topDocs, numDocs, false);
        }

        // 5. Test sort optimization on date field + long field
        {
            TestSearchContext searchContext = new TestSearchContext(searchExecutionContext, indexShard, newContextSearcher(reader));
            searchContext.sort(formatsDateLong);
            searchContext.parsedQuery(query);
            searchContext.setTask(task);
            searchContext.trackTotalHitsUpTo(10);
            searchContext.setSize(10);
            QueryPhase.executeInternal(searchContext);
            assertTrue(searchContext.sort().sort.getSort()[0].getOptimizeSortWithPoints());
            assertSortResults(searchContext.queryResult().topDocs().topDocs, (long) numDocs, true);
        }

        // 6. Test sort optimization on when from > 0 and size = 0
        {
            TestSearchContext searchContext = new TestSearchContext(searchExecutionContext, indexShard, newContextSearcher(reader));
            searchContext.sort(formatsLong);
            searchContext.parsedQuery(query);
            searchContext.setTask(task);
            searchContext.trackTotalHitsUpTo(10);
            searchContext.from(5);
            searchContext.setSize(0);
            QueryPhase.executeInternal(searchContext);
            assertTrue(searchContext.sort().sort.getSort()[0].getOptimizeSortWithPoints());
            assertThat(searchContext.queryResult().topDocs().topDocs.scoreDocs, arrayWithSize(0));
            assertThat(searchContext.queryResult().topDocs().topDocs.totalHits.value, equalTo((long) numDocs));
        }

        // 7. Test that sort optimization doesn't break a case where from = 0 and size= 0
        {
            TestSearchContext searchContext = new TestSearchContext(searchExecutionContext, indexShard, newContextSearcher(reader));
            searchContext.sort(formatsLong);
            searchContext.parsedQuery(query);
            searchContext.setTask(task);
            searchContext.setSize(0);
            QueryPhase.executeInternal(searchContext);
        }

        reader.close();
        dir.close();
    }

    public void testMaxScoreQueryVisitor() {
        BitSetProducer producer = context -> new FixedBitSet(1);
        Query query = new ESToParentBlockJoinQuery(new MatchAllDocsQuery(), producer, ScoreMode.Avg, "nested");
        assertTrue(hasInfMaxScore(query));

        query = new ESToParentBlockJoinQuery(new MatchAllDocsQuery(), producer, ScoreMode.None, "nested");
        assertFalse(hasInfMaxScore(query));

        for (Occur occur : Occur.values()) {
            query = new BooleanQuery.Builder().add(
                new ESToParentBlockJoinQuery(new MatchAllDocsQuery(), producer, ScoreMode.Avg, "nested"),
                occur
            ).build();
            if (occur == Occur.MUST) {
                assertTrue(hasInfMaxScore(query));
            } else {
                assertFalse(hasInfMaxScore(query));
            }

            query = new BooleanQuery.Builder().add(
                new BooleanQuery.Builder().add(
                    new ESToParentBlockJoinQuery(new MatchAllDocsQuery(), producer, ScoreMode.Avg, "nested"),
                    occur
                ).build(),
                occur
            ).build();
            if (occur == Occur.MUST) {
                assertTrue(hasInfMaxScore(query));
            } else {
                assertFalse(hasInfMaxScore(query));
            }

            query = new BooleanQuery.Builder().add(
                new BooleanQuery.Builder().add(
                    new ESToParentBlockJoinQuery(new MatchAllDocsQuery(), producer, ScoreMode.Avg, "nested"),
                    occur
                ).build(),
                Occur.FILTER
            ).build();
            assertFalse(hasInfMaxScore(query));

            query = new BooleanQuery.Builder().add(
                new BooleanQuery.Builder().add(new SpanTermQuery(new Term("field", "foo")), occur)
                    .add(new ESToParentBlockJoinQuery(new MatchAllDocsQuery(), producer, ScoreMode.Avg, "nested"), occur)
                    .build(),
                occur
            ).build();
            if (occur == Occur.MUST) {
                assertTrue(hasInfMaxScore(query));
            } else {
                assertFalse(hasInfMaxScore(query));
            }
        }
    }

    // assert score docs are in order and their number is as expected
    private void assertSortResults(TopDocs topDocs, long totalNumDocs, boolean isDoubleSort) {
        assertEquals(TotalHits.Relation.GREATER_THAN_OR_EQUAL_TO, topDocs.totalHits.relation);
        assertThat(topDocs.totalHits.value, lessThan(totalNumDocs)); // we collected less docs than total number
        long cur1, cur2;
        long prev1 = Long.MIN_VALUE;
        long prev2 = Long.MIN_VALUE;
        for (ScoreDoc scoreDoc : topDocs.scoreDocs) {
            cur1 = (long) ((FieldDoc) scoreDoc).fields[0];
            assertThat(cur1, greaterThan(prev1)); // test that docs are properly sorted on the first sort
            if (isDoubleSort) {
                cur2 = (long) ((FieldDoc) scoreDoc).fields[1];
                if (cur1 == prev1) {
                    assertThat(cur2, greaterThan(prev2)); // test that docs are properly sorted on the secondary sort
                }
                prev2 = cur2;
            }
            prev1 = cur1;
        }
    }

    public void testMinScore() throws Exception {
        Directory dir = newDirectory();
        IndexWriterConfig iwc = newIndexWriterConfig();
        RandomIndexWriter w = new RandomIndexWriter(random(), dir, iwc);
        for (int i = 0; i < 10; i++) {
            Document doc = new Document();
            doc.add(new StringField("foo", "bar", Store.NO));
            doc.add(new StringField("filter", "f1", Store.NO));
            w.addDocument(doc);
        }
        w.close();

        IndexReader reader = DirectoryReader.open(dir);
        TestSearchContext context = new TestSearchContext(null, indexShard, newContextSearcher(reader));
        context.parsedQuery(
            new ParsedQuery(
                new BooleanQuery.Builder().add(new TermQuery(new Term("foo", "bar")), Occur.MUST)
                    .add(new TermQuery(new Term("filter", "f1")), Occur.SHOULD)
                    .build()
            )
        );
        context.minimumScore(0.01f);
        context.setTask(new SearchShardTask(123L, "", "", "", null, Collections.emptyMap()));
        context.setSize(1);
        context.trackTotalHitsUpTo(5);

        QueryPhase.executeInternal(context);
        assertEquals(10, context.queryResult().topDocs().topDocs.totalHits.value);

        reader.close();
        dir.close();
    }

    public void testCancellationDuringRewrite() throws IOException {
        try (Directory dir = newDirectory(); RandomIndexWriter w = new RandomIndexWriter(random(), dir, newIndexWriterConfig())) {

            for (int i = 0; i < 10; i++) {
                Document doc = new Document();
                doc.add(new StringField("foo", "a".repeat(i), Store.NO));
                w.addDocument(doc);
            }
            w.flush();
            w.close();

            try (IndexReader reader = DirectoryReader.open(dir)) {
                TestSearchContext context = new TestSearchContext(null, indexShard, newContextSearcher(reader));
                PrefixQuery prefixQuery = new PrefixQuery(new Term("foo", "a"));
                prefixQuery.setRewriteMethod(MultiTermQuery.SCORING_BOOLEAN_REWRITE);
                context.parsedQuery(new ParsedQuery(prefixQuery));
                SearchShardTask task = new SearchShardTask(randomLong(), "transport", "", "", TaskId.EMPTY_TASK_ID, Collections.emptyMap());
                TaskCancelHelper.cancel(task, "simulated");
                context.setTask(task);
                context.searcher().addQueryCancellation(task::ensureNotCancelled);
                expectThrows(TaskCancelledException.class, () -> context.rewrittenQuery());
            }
        }
    }

    private static ContextIndexSearcher newContextSearcher(IndexReader reader) throws IOException {
        return new ContextIndexSearcher(
            reader,
            IndexSearcher.getDefaultSimilarity(),
            IndexSearcher.getDefaultQueryCache(),
            IndexSearcher.getDefaultQueryCachingPolicy(),
            true
        );
    }

    private static ContextIndexSearcher newEarlyTerminationContextSearcher(IndexReader reader, int size) throws IOException {
        return new ContextIndexSearcher(
            reader,
            IndexSearcher.getDefaultSimilarity(),
            IndexSearcher.getDefaultQueryCache(),
            IndexSearcher.getDefaultQueryCachingPolicy(),
            true
        ) {

            @Override
            public void search(List<LeafReaderContext> leaves, Weight weight, Collector collector) throws IOException {
                final Collector in = new AssertingEarlyTerminationFilterCollector(collector, size);
                super.search(leaves, weight, in);
            }
        };
    }

    private static class AssertingEarlyTerminationFilterCollector extends FilterCollector {
        private final int size;

        AssertingEarlyTerminationFilterCollector(Collector in, int size) {
            super(in);
            this.size = size;
        }

        @Override
        public LeafCollector getLeafCollector(LeafReaderContext context) throws IOException {
            final LeafCollector in = super.getLeafCollector(context);
            return new FilterLeafCollector(in) {
                int collected;

                @Override
                public void collect(int doc) throws IOException {
                    assert collected <= size : "should not collect more than " + size + " doc per segment, got " + collected;
                    ++collected;
                    super.collect(doc);
                }
            };
        }
    }
}
