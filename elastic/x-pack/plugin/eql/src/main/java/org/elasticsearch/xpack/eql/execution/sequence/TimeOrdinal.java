/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.eql.execution.sequence;

import org.elasticsearch.xpack.ql.capabilities.Resolvable;
import org.elasticsearch.xpack.ql.expression.Attribute;

import java.util.List;
import java.util.Objects;

import static java.util.Arrays.asList;
import static java.util.Collections.singletonList;

/**
 * Time ordinal for a given event.
 * It is an internal construct that wraps the mandatory timestamp attribute and the optional application tiebreaker.
 */
public class TimeOrdinal implements Resolvable {

    private final Attribute timestamp;
    private final Attribute tiebreaker;

    public TimeOrdinal(Attribute timestamp, Attribute tiebreaker) {
        this.timestamp = timestamp;
        this.tiebreaker = tiebreaker;
    }
    
    @Override
    public int hashCode() {
        return Objects.hash(timestamp, tiebreaker);
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }
        
        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }
        
        TimeOrdinal other = (TimeOrdinal) obj;
        return Objects.equals(timestamp, other.timestamp) &&
                Objects.equals(tiebreaker, other.tiebreaker);
    }

    @Override
    public boolean resolved() {
        return timestamp.resolved() && (tiebreaker == null || tiebreaker.resolved());
    }

    public List<Attribute> output() {
        return tiebreaker == null ? singletonList(timestamp) : asList(timestamp, tiebreaker);
    }
}
