/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.unsignedlong;

import org.elasticsearch.script.field.Field;

import java.math.BigInteger;
import java.util.List;

public interface UnsignedLongField extends Field<Long> {

    /** Returns the 0th index value as an {@code long} if it exists, otherwise {@code defaultValue}. */
    long getLong(long defaultValue);

    /** Returns the value at {@code index} as an {@code long} if it exists, otherwise {@code defaultValue}. */
    long getLong(int index, long defaultValue);

    /** Returns the 0th index value as a {@code BigInteger} if it exists, otherwise {@code defaultValue}. */
    BigInteger getBigInteger(BigInteger defaultValue);

    /** Returns the value at {@code index} as a {@code BigInteger} if it exists, otherwise {@code defaultValue}. */
    BigInteger getBigInteger(int index, BigInteger defaultValue);

    /** Converts all the values to {@code BigInteger} and returns them as a {@code List}. */
    List<BigInteger> getBigIntegers();
}
