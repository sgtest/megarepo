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

package org.elasticsearch.search.fetch.subphase;

import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.search.FieldDoc;
import org.apache.lucene.search.ScoreDoc;
import org.elasticsearch.common.lucene.search.TopDocsAndMaxScore;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.SearchHits;
import org.elasticsearch.search.fetch.FetchContext;
import org.elasticsearch.search.fetch.FetchPhase;
import org.elasticsearch.search.fetch.FetchSearchResult;
import org.elasticsearch.search.fetch.FetchSubPhase;
import org.elasticsearch.search.fetch.FetchSubPhaseProcessor;
import org.elasticsearch.search.lookup.SourceLookup;

import java.io.IOException;
import java.util.HashMap;
import java.util.Map;

public final class InnerHitsPhase implements FetchSubPhase {

    private final FetchPhase fetchPhase;

    public InnerHitsPhase(FetchPhase fetchPhase) {
        this.fetchPhase = fetchPhase;
    }

    @Override
    public FetchSubPhaseProcessor getProcessor(FetchContext searchContext) {
        if (searchContext.innerHits() == null) {
            return null;
        }
        Map<String, InnerHitsContext.InnerHitSubContext> innerHits = searchContext.innerHits().getInnerHits();
        return new FetchSubPhaseProcessor() {
            @Override
            public void setNextReader(LeafReaderContext readerContext) {

            }

            @Override
            public void process(HitContext hitContext) throws IOException {
                hitExecute(innerHits, hitContext);
            }
        };
    }

    private void hitExecute(Map<String, InnerHitsContext.InnerHitSubContext> innerHits, HitContext hitContext) throws IOException {

        SearchHit hit = hitContext.hit();
        SourceLookup sourceLookup = hitContext.sourceLookup();

        for (Map.Entry<String, InnerHitsContext.InnerHitSubContext> entry : innerHits.entrySet()) {
            InnerHitsContext.InnerHitSubContext innerHitsContext = entry.getValue();
            TopDocsAndMaxScore topDoc = innerHitsContext.topDocs(hit);

            Map<String, SearchHits> results = hit.getInnerHits();
            if (results == null) {
                hit.setInnerHits(results = new HashMap<>());
            }
            innerHitsContext.queryResult().topDocs(topDoc, innerHitsContext.sort() == null ? null : innerHitsContext.sort().formats);
            int[] docIdsToLoad = new int[topDoc.topDocs.scoreDocs.length];
            for (int j = 0; j < topDoc.topDocs.scoreDocs.length; j++) {
                docIdsToLoad[j] = topDoc.topDocs.scoreDocs[j].doc;
            }
            innerHitsContext.docIdsToLoad(docIdsToLoad, docIdsToLoad.length);
            innerHitsContext.setRootId(hit.getId());
            innerHitsContext.setRootLookup(sourceLookup);

            fetchPhase.execute(innerHitsContext);
            FetchSearchResult fetchResult = innerHitsContext.fetchResult();
            SearchHit[] internalHits = fetchResult.fetchResult().hits().getHits();
            for (int j = 0; j < internalHits.length; j++) {
                ScoreDoc scoreDoc = topDoc.topDocs.scoreDocs[j];
                SearchHit searchHitFields = internalHits[j];
                searchHitFields.score(scoreDoc.score);
                if (scoreDoc instanceof FieldDoc) {
                    FieldDoc fieldDoc = (FieldDoc) scoreDoc;
                    searchHitFields.sortValues(fieldDoc.fields, innerHitsContext.sort().formats);
                }
            }
            results.put(entry.getKey(), fetchResult.hits());
        }
    }
}
