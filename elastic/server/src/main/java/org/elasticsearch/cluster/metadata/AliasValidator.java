/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster.metadata;

import org.elasticsearch.action.admin.indices.alias.Alias;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.xcontent.LoggingDeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.index.query.Rewriteable;
import org.elasticsearch.indices.InvalidAliasNameException;

import java.io.IOException;
import java.io.InputStream;
import java.util.function.Function;

import static org.elasticsearch.index.query.AbstractQueryBuilder.parseInnerQueryBuilder;

/**
 * Validator for an alias, to be used before adding an alias to the index metadata
 * and make sure the alias is valid
 */
public class AliasValidator {
    /**
     * Allows to validate an {@link org.elasticsearch.action.admin.indices.alias.Alias} and make sure
     * it's valid before it gets added to the index metadata. Doesn't validate the alias filter.
     * @throws IllegalArgumentException if the alias is not valid
     */
    public void validateAlias(Alias alias, String index, Metadata metadata) {
        validateAlias(alias.name(), index, alias.indexRouting(), metadata::index);
    }

    /**
     * Allows to validate an {@link org.elasticsearch.cluster.metadata.AliasMetadata} and make sure
     * it's valid before it gets added to the index metadata. Doesn't validate the alias filter.
     * @throws IllegalArgumentException if the alias is not valid
     */
    public void validateAliasMetadata(AliasMetadata aliasMetadata, String index, Metadata metadata) {
        validateAlias(aliasMetadata.alias(), index, aliasMetadata.indexRouting(), metadata::index);
    }

    /**
     * Allows to partially validate an alias, without knowing which index it'll get applied to.
     * Useful with index templates containing aliases. Checks also that it is possible to parse
     * the alias filter via {@link org.elasticsearch.common.xcontent.XContentParser},
     * without validating it as a filter though.
     * @throws IllegalArgumentException if the alias is not valid
     */
    public void validateAliasStandalone(Alias alias) {
        validateAliasStandalone(alias.name(), alias.indexRouting());
        if (Strings.hasLength(alias.filter())) {
            try {
                XContentHelper.convertToMap(XContentFactory.xContent(alias.filter()), alias.filter(), false);
            } catch (Exception e) {
                throw new IllegalArgumentException("failed to parse filter for alias [" + alias.name() + "]", e);
            }
        }
    }

    /**
     * Validate a proposed alias.
     */
    public void validateAlias(String alias, String index, @Nullable String indexRouting, Function<String, IndexMetadata> indexLookup) {
        validateAliasStandalone(alias, indexRouting);

        if (Strings.hasText(index) == false) {
            throw new IllegalArgumentException("index name is required");
        }

        IndexMetadata indexNamedSameAsAlias = indexLookup.apply(alias);
        if (indexNamedSameAsAlias != null) {
            throw new InvalidAliasNameException(indexNamedSameAsAlias.getIndex(), alias, "an index exists with the same name as the alias");
        }
    }

    void validateAliasStandalone(String alias, String indexRouting) {
        if (Strings.hasText(alias) == false) {
            throw new IllegalArgumentException("alias name is required");
        }
        MetadataCreateIndexService.validateIndexOrAliasName(alias, InvalidAliasNameException::new);
        if (indexRouting != null && indexRouting.indexOf(',') != -1) {
            throw new IllegalArgumentException("alias [" + alias + "] has several index routing values associated with it");
        }
    }

    /**
     * Validates an alias filter by parsing it using the
     * provided {@link SearchExecutionContext}
     * @throws IllegalArgumentException if the filter is not valid
     */
    public void validateAliasFilter(String alias, String filter, SearchExecutionContext searchExecutionContext,
            NamedXContentRegistry xContentRegistry) {
        assert searchExecutionContext != null;
        try (XContentParser parser = XContentFactory.xContent(filter)
            .createParser(xContentRegistry, LoggingDeprecationHandler.INSTANCE, filter)) {
            validateAliasFilter(parser, searchExecutionContext);
        } catch (Exception e) {
            throw new IllegalArgumentException("failed to parse filter for alias [" + alias + "]", e);
        }
    }

    /**
     * Validates an alias filter by parsing it using the
     * provided {@link SearchExecutionContext}
     * @throws IllegalArgumentException if the filter is not valid
     */
    public void validateAliasFilter(String alias, BytesReference filter, SearchExecutionContext searchExecutionContext,
                                    NamedXContentRegistry xContentRegistry) {
        assert searchExecutionContext != null;

        try (InputStream inputStream = filter.streamInput();
             XContentParser parser = XContentFactory.xContentType(inputStream).xContent()
                     .createParser(xContentRegistry, LoggingDeprecationHandler.INSTANCE, filter.streamInput())) {
            validateAliasFilter(parser, searchExecutionContext);
        } catch (Exception e) {
            throw new IllegalArgumentException("failed to parse filter for alias [" + alias + "]", e);
        }
    }

    private static void validateAliasFilter(XContentParser parser, SearchExecutionContext searchExecutionContext) throws IOException {
        QueryBuilder parseInnerQueryBuilder = parseInnerQueryBuilder(parser);
        QueryBuilder queryBuilder = Rewriteable.rewrite(parseInnerQueryBuilder, searchExecutionContext, true);
        queryBuilder.toQuery(searchExecutionContext);
    }
}
