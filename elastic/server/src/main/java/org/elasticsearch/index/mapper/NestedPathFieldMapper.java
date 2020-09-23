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

package org.elasticsearch.index.mapper;

import org.apache.lucene.document.Field;
import org.apache.lucene.document.FieldType;
import org.apache.lucene.index.IndexOptions;
import org.apache.lucene.index.Term;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.TermQuery;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.Version;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.query.QueryShardContext;

import java.util.Collections;

public class NestedPathFieldMapper extends MetadataFieldMapper {

    public static final String NAME_PRE_V8 = "_type";
    public static final String NAME = "_nested_path";

    public static String name(Settings settings) {
        if (Version.indexCreated(settings).before(Version.V_8_0_0)) {
            return NAME_PRE_V8;
        }
        return NAME;
    }

    public static Query filter(Settings settings, String path) {
        return new TermQuery(new Term(name(settings), new BytesRef(path)));
    }

    public static Field field(Settings settings, String path) {
        return new Field(name(settings), path, Defaults.FIELD_TYPE);
    }

    public static class Defaults {

        public static final FieldType FIELD_TYPE = new FieldType();

        static {
            FIELD_TYPE.setIndexOptions(IndexOptions.DOCS);
            FIELD_TYPE.setTokenized(false);
            FIELD_TYPE.setStored(false);
            FIELD_TYPE.setOmitNorms(true);
            FIELD_TYPE.freeze();
        }
    }

    public static final TypeParser PARSER = new FixedTypeParser(c -> {
        final IndexSettings indexSettings = c.mapperService().getIndexSettings();
        return new NestedPathFieldMapper(indexSettings.getSettings());
    });

    public static final class NestedPathFieldType extends StringFieldType {

        private NestedPathFieldType(Settings settings) {
            super(NestedPathFieldMapper.name(settings), true, false, false, TextSearchInfo.SIMPLE_MATCH_ONLY, Collections.emptyMap());
        }

        @Override
        public String typeName() {
            return NAME;
        }

        @Override
        public Query existsQuery(QueryShardContext context) {
            throw new UnsupportedOperationException("Cannot run exists() query against the nested field path");
        }
    }

    private NestedPathFieldMapper(Settings settings) {
        super(new NestedPathFieldType(settings));
    }

    @Override
    protected String contentType() {
        return NAME;
    }

}
