/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.elasticsearch.Version;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.util.StringLiteralDeduplicator;
import org.elasticsearch.xcontent.ToXContentFragment;

import java.util.Map;
import java.util.Objects;

public abstract class Mapper implements ToXContentFragment, Iterable<Mapper> {

    public abstract static class Builder {

        protected final String name;

        protected Builder(String name) {
            this.name = internFieldName(name);
        }

        public String name() {
            return this.name;
        }

        /** Returns a newly built mapper. */
        public abstract Mapper build(MapperBuilderContext context);
    }

    public interface TypeParser {
        Mapper.Builder parse(String name, Map<String, Object> node, MappingParserContext parserContext) throws MapperParsingException;

        /**
         * Whether we can parse this type on indices with the given index created version.
         */
        default boolean supportsVersion(Version indexCreatedVersion) {
            return indexCreatedVersion.onOrAfter(Version.CURRENT.minimumIndexCompatibilityVersion());
        }
    }

    private final String simpleName;

    public Mapper(String simpleName) {
        Objects.requireNonNull(simpleName);
        this.simpleName = internFieldName(simpleName);
    }

    /** Returns the simple name, which identifies this mapper against other mappers at the same level in the mappers hierarchy
     * TODO: make this protected once Mapper and FieldMapper are merged together */
    public final String simpleName() {
        return simpleName;
    }

    /** Returns the canonical name which uniquely identifies the mapper against other mappers in a type. */
    public abstract String name();

    /**
     * Returns a name representing the type of this mapper.
     */
    public abstract String typeName();

    /** Return the merge of {@code mergeWith} into this.
     *  Both {@code this} and {@code mergeWith} will be left unmodified. */
    public abstract Mapper merge(Mapper mergeWith);

    /**
     * Validate any cross-field references made by this mapper
     * @param mappers a {@link MappingLookup} that can produce references to other mappers
     */
    public abstract void validate(MappingLookup mappers);

    @Override
    public String toString() {
        return Strings.toString(this);
    }

    private static final StringLiteralDeduplicator fieldNameStringDeduplicator = new StringLiteralDeduplicator();

    /**
     * Interns the given field name string through a {@link StringLiteralDeduplicator}.
     * @param fieldName field name to intern
     * @return interned field name string
     */
    public static String internFieldName(String fieldName) {
        return fieldNameStringDeduplicator.deduplicate(fieldName);
    }
}
