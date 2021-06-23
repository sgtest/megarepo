/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.elasticsearch.common.xcontent.ToXContentFragment;

import java.util.Map;
import java.util.Objects;

public abstract class Mapper implements ToXContentFragment, Iterable<Mapper> {

    public abstract static class Builder {

        protected final String name;

        protected Builder(String name) {
            this.name = name;
        }

        public String name() {
            return this.name;
        }

        /** Returns a newly built mapper. */
        public abstract Mapper build(ContentPath contentPath);
    }

    public interface TypeParser {
        Mapper.Builder parse(String name, Map<String, Object> node, MappingParserContext parserContext) throws MapperParsingException;
    }

    private final String simpleName;

    public Mapper(String simpleName) {
        Objects.requireNonNull(simpleName);
        this.simpleName = simpleName;
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

}
