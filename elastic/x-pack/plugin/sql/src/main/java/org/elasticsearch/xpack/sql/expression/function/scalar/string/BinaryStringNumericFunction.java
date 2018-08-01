/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.string;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.type.DataType;

/**
 * A binary string function with a numeric second parameter and a string result
 */
public abstract class BinaryStringNumericFunction extends BinaryStringFunction<Number, String> {

    public BinaryStringNumericFunction(Location location, Expression left, Expression right) {
        super(location, left, right);
    }

    @Override
    protected TypeResolution resolveSecondParameterInputType(DataType inputType) {
        return inputType.isNumeric() ? 
                TypeResolution.TYPE_RESOLVED : 
                new TypeResolution("'%s' requires second parameter to be a numeric type, received %s", functionName(), inputType);
    }
    
    @Override
    public DataType dataType() {
        return DataType.KEYWORD;
    }
}
