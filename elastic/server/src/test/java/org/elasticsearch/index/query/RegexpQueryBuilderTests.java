/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.query;

import org.apache.lucene.search.Query;
import org.apache.lucene.search.RegexpQuery;
import org.elasticsearch.common.ParsingException;
import org.elasticsearch.test.AbstractQueryTestCase;

import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.instanceOf;

public class RegexpQueryBuilderTests extends AbstractQueryTestCase<RegexpQueryBuilder> {

    @Override
    protected RegexpQueryBuilder doCreateTestQueryBuilder() {
        RegexpQueryBuilder query = randomRegexpQuery();
        if (randomBoolean()) {
            List<RegexpFlag> flags = new ArrayList<>();
            int iter = randomInt(5);
            for (int i = 0; i < iter; i++) {
                flags.add(randomFrom(RegexpFlag.values()));
            }
            query.flags(flags.toArray(new RegexpFlag[flags.size()]));
        }
        if (randomBoolean()) {
            query.caseInsensitive(true);
        }
        if (randomBoolean()) {
            query.maxDeterminizedStates(randomInt(50000));
        }
        if (randomBoolean()) {
            query.rewrite(randomFrom(getRandomRewriteMethod()));
        }
        return query;
    }

    @Override
    protected Map<String, RegexpQueryBuilder> getAlternateVersions() {
        Map<String, RegexpQueryBuilder> alternateVersions = new HashMap<>();
        RegexpQueryBuilder regexpQuery = randomRegexpQuery();
        String contentString = "{\n" +
                "    \"regexp\" : {\n" +
                "        \"" + regexpQuery.fieldName() + "\" : \"" + regexpQuery.value() + "\"\n" +
                "    }\n" +
                "}";
        alternateVersions.put(contentString, regexpQuery);
        return alternateVersions;
    }

    private static RegexpQueryBuilder randomRegexpQuery() {
        // mapped or unmapped fields
        String fieldName = randomFrom(TEXT_FIELD_NAME, TEXT_ALIAS_FIELD_NAME, randomAlphaOfLengthBetween(1, 10));
        String value = randomAlphaOfLengthBetween(1, 10);
        return new RegexpQueryBuilder(fieldName, value);
    }

    @Override
    protected void doAssertLuceneQuery(RegexpQueryBuilder queryBuilder, Query query, SearchExecutionContext context) throws IOException {
        assertThat(query, instanceOf(RegexpQuery.class));
        RegexpQuery regexpQuery = (RegexpQuery) query;

        String expectedFieldName = expectedFieldName( queryBuilder.fieldName());
        assertThat(regexpQuery.getField(), equalTo(expectedFieldName));
    }

    public void testIllegalArguments() {
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> new RegexpQueryBuilder(null, "text"));
        assertEquals("field name is null or empty", e.getMessage());
        e = expectThrows(IllegalArgumentException.class, () -> new RegexpQueryBuilder("", "text"));
        assertEquals("field name is null or empty", e.getMessage());

        e = expectThrows(IllegalArgumentException.class, () -> new RegexpQueryBuilder("field", null));
        assertEquals("value cannot be null", e.getMessage());
    }

    public void testFromJson() throws IOException {
        String json =
                "{\n" +
                "  \"regexp\" : {\n" +
                "    \"name.first\" : {\n" +
                "      \"value\" : \"s.*y\",\n" +
                "      \"flags_value\" : 7,\n" +
                "      \"case_insensitive\" : true,\n" +
                "      \"max_determinized_states\" : 20000,\n" +
                "      \"boost\" : 1.0\n" +
                "    }\n" +
                "  }\n" +
                "}";

        RegexpQueryBuilder parsed = (RegexpQueryBuilder) parseQuery(json);
        checkGeneratedJson(json, parsed);

        assertEquals(json, "s.*y", parsed.value());
        assertEquals(json, 20000, parsed.maxDeterminizedStates());
    }

    public void testNumeric() throws Exception {
        RegexpQueryBuilder query = new RegexpQueryBuilder(INT_FIELD_NAME, "12");
        SearchExecutionContext context = createSearchExecutionContext();
        QueryShardException e = expectThrows(QueryShardException.class, () -> query.toQuery(context));
        assertEquals("Can only use regexp queries on keyword and text fields - not on [mapped_int] which is of type [integer]",
                e.getMessage());
    }

    public void testParseFailsWithMultipleFields() throws IOException {
        String json =
                "{\n" +
                "    \"regexp\": {\n" +
                "      \"user1\": {\n" +
                "        \"value\": \"k.*y\"\n" +
                "      },\n" +
                "      \"user2\": {\n" +
                "        \"value\": \"k.*y\"\n" +
                "      }\n" +
                "    }\n" +
                "}";
        ParsingException e = expectThrows(ParsingException.class, () -> parseQuery(json));
        assertEquals("[regexp] query doesn't support multiple fields, found [user1] and [user2]", e.getMessage());

        String shortJson =
                "{\n" +
                "    \"regexp\": {\n" +
                "      \"user1\": \"k.*y\",\n" +
                "      \"user2\": \"k.*y\"\n" +
                "    }\n" +
                "}";
        e = expectThrows(ParsingException.class, () -> parseQuery(shortJson));
        assertEquals("[regexp] query doesn't support multiple fields, found [user1] and [user2]", e.getMessage());
    }
}
