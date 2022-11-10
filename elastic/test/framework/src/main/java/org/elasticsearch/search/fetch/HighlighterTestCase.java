/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.fetch;

import org.apache.lucene.search.IndexSearcher;
import org.elasticsearch.common.text.Text;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.mapper.MapperServiceTestCase;
import org.elasticsearch.index.mapper.ParsedDocument;
import org.elasticsearch.index.query.ParsedQuery;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.fetch.subphase.highlight.FastVectorHighlighter;
import org.elasticsearch.search.fetch.subphase.highlight.HighlightField;
import org.elasticsearch.search.fetch.subphase.highlight.HighlightPhase;
import org.elasticsearch.search.fetch.subphase.highlight.Highlighter;
import org.elasticsearch.search.fetch.subphase.highlight.PlainHighlighter;
import org.elasticsearch.search.fetch.subphase.highlight.UnifiedHighlighter;
import org.elasticsearch.search.lookup.Source;

import java.io.IOException;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class HighlighterTestCase extends MapperServiceTestCase {

    protected Map<String, Highlighter> getHighlighters() {
        return Map.of(
            "unified",
            new UnifiedHighlighter(),
            "fvh",
            new FastVectorHighlighter(getIndexSettings()),
            "plain",
            new PlainHighlighter()
        );
    }

    /**
     * Runs the highlight phase for a search over a specific document
     * @param mapperService the Mappings to use for highlighting
     * @param doc           a parsed document to highlight
     * @param search        the search to highlight
     */
    protected final Map<String, HighlightField> highlight(MapperService mapperService, ParsedDocument doc, SearchSourceBuilder search)
        throws IOException {
        Map<String, HighlightField> highlights = new HashMap<>();
        withLuceneIndex(mapperService, iw -> iw.addDocument(doc.rootDoc()), ir -> {
            SearchExecutionContext context = createSearchExecutionContext(mapperService, new IndexSearcher(ir));
            HighlightPhase highlightPhase = new HighlightPhase(getHighlighters());
            FetchSubPhaseProcessor processor = highlightPhase.getProcessor(fetchContext(context, search));
            Source source = Source.fromBytes(doc.source());
            FetchSubPhase.HitContext hitContext = new FetchSubPhase.HitContext(
                new SearchHit(0, "id"),
                ir.leaves().get(0),
                0,
                Map.of(),
                source
            );
            processor.process(hitContext);
            highlights.putAll(hitContext.hit().getHighlightFields());
        });
        return highlights;
    }

    /**
     * Given a set of highlights, assert that any particular field has the expected fragments
     */
    protected static void assertHighlights(Map<String, HighlightField> highlights, String field, String... fragments) {
        assertNotNull("No highlights reported for field [" + field + "]", highlights.get(field));
        List<String> actualFragments = Arrays.stream(highlights.get(field).getFragments()).map(Text::toString).collect(Collectors.toList());
        List<String> expectedFragments = List.of(fragments);
        assertEquals(expectedFragments, actualFragments);
    }

    private static FetchContext fetchContext(SearchExecutionContext context, SearchSourceBuilder search) throws IOException {
        FetchContext fetchContext = mock(FetchContext.class);
        when(fetchContext.highlight()).thenReturn(search.highlighter().build(context));
        when(fetchContext.parsedQuery()).thenReturn(new ParsedQuery(search.query().toQuery(context)));
        when(fetchContext.getSearchExecutionContext()).thenReturn(context);
        when(fetchContext.sourceLoader()).thenReturn(context.newSourceLoader(false));
        return fetchContext;
    }
}
