/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.support;

import org.apache.lucene.search.Query;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.index.query.BoolQueryBuilder;
import org.elasticsearch.index.query.ExistsQueryBuilder;
import org.elasticsearch.index.query.IdsQueryBuilder;
import org.elasticsearch.index.query.MatchAllQueryBuilder;
import org.elasticsearch.index.query.MatchNoneQueryBuilder;
import org.elasticsearch.index.query.MatchQueryBuilder;
import org.elasticsearch.index.query.PrefixQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.index.query.QueryRewriteContext;
import org.elasticsearch.index.query.RangeQueryBuilder;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.index.query.SimpleQueryStringBuilder;
import org.elasticsearch.index.query.TermQueryBuilder;
import org.elasticsearch.index.query.TermsQueryBuilder;
import org.elasticsearch.index.query.WildcardQueryBuilder;
import org.elasticsearch.index.search.QueryParserHelper;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.AuthenticationField;
import org.elasticsearch.xpack.security.authc.ApiKeyService;

import java.io.IOException;
import java.util.HashMap;
import java.util.Map;
import java.util.Set;
import java.util.function.Consumer;

import static org.elasticsearch.xpack.security.action.apikey.TransportQueryApiKeyAction.API_KEY_TYPE_RUNTIME_MAPPING_FIELD;

public class ApiKeyBoolQueryBuilder extends BoolQueryBuilder {

    // Field names allowed at the index level
    private static final Set<String> ALLOWED_EXACT_INDEX_FIELD_NAMES = Set.of(
        "_id",
        "doc_type",
        "name",
        "type",
        API_KEY_TYPE_RUNTIME_MAPPING_FIELD,
        "api_key_invalidated",
        "invalidation_time",
        "creation_time",
        "expiration_time",
        "metadata_flattened",
        "creator.principal",
        "creator.realm"
    );

    private ApiKeyBoolQueryBuilder() {}

    /**
     * Build a bool query that is specialised for query API keys information from the security index.
     * The method processes the given QueryBuilder to ensure:
     *   * Only fields from an allowlist are queried
     *   * Only query types from an allowlist are used
     *   * Field names used in the Query DSL get translated into corresponding names used at the index level.
     *     This helps decouple the user facing and implementation level changes.
     *   * User's security context gets applied when necessary
     *   * Not exposing any other types of documents stored in the same security index
     *
     * @param queryBuilder This represents the query parsed directly from the user input. It is validated
     *                     and transformed (see above).
     * @param fieldNameVisitor This {@code Consumer} is invoked with all the (index-level) field names referred to in the passed-in query.
     * @param authentication The user's authentication object. If present, it will be used to filter the results
     *                       to only include API keys owned by the user.
     * @return A specialised query builder for API keys that is safe to run on the security index.
     */
    public static ApiKeyBoolQueryBuilder build(
        QueryBuilder queryBuilder,
        Consumer<String> fieldNameVisitor,
        @Nullable Authentication authentication
    ) {
        final ApiKeyBoolQueryBuilder finalQuery = new ApiKeyBoolQueryBuilder();
        if (queryBuilder != null) {
            QueryBuilder processedQuery = doProcess(queryBuilder, fieldNameVisitor);
            finalQuery.must(processedQuery);
        }
        finalQuery.filter(QueryBuilders.termQuery("doc_type", "api_key"));
        fieldNameVisitor.accept("doc_type");

        if (authentication != null) {
            if (authentication.isApiKey()) {
                final String apiKeyId = (String) authentication.getAuthenticatingSubject()
                    .getMetadata()
                    .get(AuthenticationField.API_KEY_ID_KEY);
                assert apiKeyId != null : "api key id must be present in the metadata";
                finalQuery.filter(QueryBuilders.idsQuery().addIds(apiKeyId));
            } else {
                finalQuery.filter(QueryBuilders.termQuery("creator.principal", authentication.getEffectiveSubject().getUser().principal()));
                fieldNameVisitor.accept("creator.principal");
                final String[] realms = ApiKeyService.getOwnersRealmNames(authentication);
                final QueryBuilder realmsQuery = ApiKeyService.filterForRealmNames(realms);
                fieldNameVisitor.accept("creator.realm");
                assert realmsQuery != null;
                finalQuery.filter(realmsQuery);
            }
        }
        return finalQuery;
    }

    private static QueryBuilder doProcess(QueryBuilder qb, Consumer<String> fieldNameVisitor) {
        if (qb instanceof final BoolQueryBuilder query) {
            final BoolQueryBuilder newQuery = QueryBuilders.boolQuery()
                .minimumShouldMatch(query.minimumShouldMatch())
                .adjustPureNegative(query.adjustPureNegative())
                .boost(query.boost());
            query.must().stream().map(q -> ApiKeyBoolQueryBuilder.doProcess(q, fieldNameVisitor)).forEach(newQuery::must);
            query.should().stream().map(q -> ApiKeyBoolQueryBuilder.doProcess(q, fieldNameVisitor)).forEach(newQuery::should);
            query.mustNot().stream().map(q -> ApiKeyBoolQueryBuilder.doProcess(q, fieldNameVisitor)).forEach(newQuery::mustNot);
            query.filter().stream().map(q -> ApiKeyBoolQueryBuilder.doProcess(q, fieldNameVisitor)).forEach(newQuery::filter);
            return newQuery;
        } else if (qb instanceof MatchAllQueryBuilder) {
            return qb;
        } else if (qb instanceof IdsQueryBuilder) {
            return qb;
        } else if (qb instanceof final TermQueryBuilder query) {
            final String translatedFieldName = ApiKeyFieldNameTranslators.translate(query.fieldName());
            fieldNameVisitor.accept(translatedFieldName);
            return QueryBuilders.termQuery(translatedFieldName, query.value())
                .caseInsensitive(query.caseInsensitive())
                .boost(query.boost());
        } else if (qb instanceof final ExistsQueryBuilder query) {
            final String translatedFieldName = ApiKeyFieldNameTranslators.translate(query.fieldName());
            fieldNameVisitor.accept(translatedFieldName);
            return QueryBuilders.existsQuery(translatedFieldName).boost(query.boost());
        } else if (qb instanceof final TermsQueryBuilder query) {
            if (query.termsLookup() != null) {
                throw new IllegalArgumentException("terms query with terms lookup is not supported for API Key query");
            }
            final String translatedFieldName = ApiKeyFieldNameTranslators.translate(query.fieldName());
            fieldNameVisitor.accept(translatedFieldName);
            return QueryBuilders.termsQuery(translatedFieldName, query.getValues()).boost(query.boost());
        } else if (qb instanceof final PrefixQueryBuilder query) {
            final String translatedFieldName = ApiKeyFieldNameTranslators.translate(query.fieldName());
            fieldNameVisitor.accept(translatedFieldName);
            return QueryBuilders.prefixQuery(translatedFieldName, query.value())
                .caseInsensitive(query.caseInsensitive())
                .rewrite(query.rewrite())
                .boost(query.boost());
        } else if (qb instanceof final WildcardQueryBuilder query) {
            final String translatedFieldName = ApiKeyFieldNameTranslators.translate(query.fieldName());
            fieldNameVisitor.accept(translatedFieldName);
            return QueryBuilders.wildcardQuery(translatedFieldName, query.value())
                .caseInsensitive(query.caseInsensitive())
                .rewrite(query.rewrite())
                .boost(query.boost());
        } else if (qb instanceof final MatchQueryBuilder query) {
            final String translatedFieldName = ApiKeyFieldNameTranslators.translate(query.fieldName());
            fieldNameVisitor.accept(translatedFieldName);
            final MatchQueryBuilder matchQueryBuilder = QueryBuilders.matchQuery(translatedFieldName, query.value());
            if (query.operator() != null) {
                matchQueryBuilder.operator(query.operator());
            }
            if (query.analyzer() != null) {
                matchQueryBuilder.analyzer(query.analyzer());
            }
            if (query.fuzziness() != null) {
                matchQueryBuilder.fuzziness(query.fuzziness());
            }
            if (query.minimumShouldMatch() != null) {
                matchQueryBuilder.minimumShouldMatch(query.minimumShouldMatch());
            }
            if (query.fuzzyRewrite() != null) {
                matchQueryBuilder.fuzzyRewrite(query.fuzzyRewrite());
            }
            if (query.zeroTermsQuery() != null) {
                matchQueryBuilder.zeroTermsQuery(query.zeroTermsQuery());
            }
            matchQueryBuilder.prefixLength(query.prefixLength())
                .maxExpansions(query.maxExpansions())
                .fuzzyTranspositions(query.fuzzyTranspositions())
                .lenient(query.lenient())
                .autoGenerateSynonymsPhraseQuery(query.autoGenerateSynonymsPhraseQuery())
                .boost(query.boost());
            return matchQueryBuilder;
        } else if (qb instanceof final RangeQueryBuilder query) {
            if (query.relation() != null) {
                throw new IllegalArgumentException("range query with relation is not supported for API Key query");
            }
            final String translatedFieldName = ApiKeyFieldNameTranslators.translate(query.fieldName());
            fieldNameVisitor.accept(translatedFieldName);
            final RangeQueryBuilder newQuery = QueryBuilders.rangeQuery(translatedFieldName);
            if (query.format() != null) {
                newQuery.format(query.format());
            }
            if (query.timeZone() != null) {
                newQuery.timeZone(query.timeZone());
            }
            if (query.from() != null) {
                newQuery.from(query.from()).includeLower(query.includeLower());
            }
            if (query.to() != null) {
                newQuery.to(query.to()).includeUpper(query.includeUpper());
            }
            return newQuery.boost(query.boost());
        } else if (qb instanceof final SimpleQueryStringBuilder simpleQueryStringBuilder) {
            if (simpleQueryStringBuilder.fields().isEmpty()) {
                simpleQueryStringBuilder.field("*");
            }
            // override lenient if querying all the fields, because, due to different field mappings,
            // the query parsing will almost certainly fail otherwise
            if (QueryParserHelper.hasAllFieldsWildcard(simpleQueryStringBuilder.fields().keySet())) {
                simpleQueryStringBuilder.lenient(true);
            }
            Map<String, Float> requestedFields = new HashMap<>(simpleQueryStringBuilder.fields());
            simpleQueryStringBuilder.fields().clear();
            for (Map.Entry<String, Float> requestedFieldNameOrPattern : requestedFields.entrySet()) {
                for (String translatedField : ApiKeyFieldNameTranslators.translatePattern(requestedFieldNameOrPattern.getKey())) {
                    simpleQueryStringBuilder.fields()
                        .compute(
                            translatedField,
                            (k, v) -> (v == null) ? requestedFieldNameOrPattern.getValue() : v * requestedFieldNameOrPattern.getValue()
                        );
                    fieldNameVisitor.accept(translatedField);
                }
            }
            if (simpleQueryStringBuilder.fields().isEmpty()) {
                // A SimpleQueryStringBuilder with empty fields() will eventually produce a SimpleQueryString query
                // that accesses all the fields, including disallowed ones.
                // Instead, the behavior we're after is that a query that accesses only disallowed fields should
                // not match any docs.
                return new MatchNoneQueryBuilder();
            } else {
                return simpleQueryStringBuilder;
            }
        } else {
            throw new IllegalArgumentException("Query type [" + qb.getName() + "] is not supported for API Key query");
        }
    }

    @Override
    protected Query doToQuery(SearchExecutionContext context) throws IOException {
        context.setAllowedFields(ApiKeyBoolQueryBuilder::isIndexFieldNameAllowed);
        return super.doToQuery(context);
    }

    @Override
    protected QueryBuilder doRewrite(QueryRewriteContext queryRewriteContext) throws IOException {
        if (queryRewriteContext instanceof SearchExecutionContext) {
            ((SearchExecutionContext) queryRewriteContext).setAllowedFields(ApiKeyBoolQueryBuilder::isIndexFieldNameAllowed);
        }
        return super.doRewrite(queryRewriteContext);
    }

    static boolean isIndexFieldNameAllowed(String fieldName) {
        return ALLOWED_EXACT_INDEX_FIELD_NAMES.contains(fieldName) || fieldName.startsWith("metadata_flattened.");
    }

}
