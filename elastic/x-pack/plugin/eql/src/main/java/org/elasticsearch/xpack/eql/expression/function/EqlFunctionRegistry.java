/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.eql.expression.function;

import org.elasticsearch.xpack.eql.expression.function.scalar.string.Substring;
import org.elasticsearch.xpack.ql.expression.function.FunctionDefinition;
import org.elasticsearch.xpack.ql.expression.function.FunctionRegistry;

import java.util.Locale;

public class EqlFunctionRegistry extends FunctionRegistry {

    public EqlFunctionRegistry() {
        super(functions());
    }
    
    private static FunctionDefinition[][] functions() {
        return new FunctionDefinition[][] {
        // Scalar functions
        // String
            new FunctionDefinition[] {
                def(Substring.class, Substring::new, "substring"),
            },
        };
    }

    @Override
    protected String normalize(String name) {
        return name.toLowerCase(Locale.ROOT);
    }
}
