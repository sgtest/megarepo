/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition;

import org.elasticsearch.xpack.sql.SqlIllegalArgumentException;
import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.runtime.Processor;
import org.elasticsearch.xpack.sql.tree.Location;

public abstract class NonExecutableInput<T> extends LeafInput<T> {
    NonExecutableInput(Location location, Expression expression, T context) {
        super(location, expression, context);
    }

    @Override
    public boolean resolved() {
        return false;
    }

    @Override
    public Processor asProcessor() {
        throw new SqlIllegalArgumentException("Unresolved input - needs resolving first");
    }
}
