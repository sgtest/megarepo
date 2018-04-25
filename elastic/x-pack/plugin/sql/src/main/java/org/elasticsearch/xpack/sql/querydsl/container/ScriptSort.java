/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.querydsl.container;

import java.util.Objects;

import org.elasticsearch.xpack.sql.expression.function.scalar.script.ScriptTemplate;

public class ScriptSort extends Sort {

    private final ScriptTemplate script;

    public ScriptSort(ScriptTemplate script, Direction direction) {
        super(direction);
        this.script = script;
    }

    public ScriptTemplate script() {
        return script;
    }

    @Override
    public int hashCode() {
        return Objects.hash(direction(), script);
    }
    
    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }
        
        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }
        
        ScriptSort other = (ScriptSort) obj;
        return Objects.equals(direction(), other.direction())
                && Objects.equals(script, other.script);
    }
}
