/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.eql.execution.search;

import org.elasticsearch.search.SearchHit;

import java.util.Objects;

public class HitReference {

    private final String index;
    private final String id;

    public HitReference(String index, String id) {
        this.index = index;
        this.id = id;
    }

    public HitReference(SearchHit hit) {
        this(hit.getIndex(), hit.getId());
    }

    public String index() {
        return index;
    }

    public String id() {
        return id;
    }

    @Override
    public int hashCode() {
        return Objects.hash(index, id);
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }

        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }

        HitReference other = (HitReference) obj;
        return Objects.equals(index, other.index)
                && Objects.equals(id, other.id);
    }

    @Override
    public String toString() {
        return "doc[" + index + "][" + id + "]";
    }
}
