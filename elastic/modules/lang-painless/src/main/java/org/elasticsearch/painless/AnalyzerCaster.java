/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.painless;

import org.elasticsearch.painless.Definition.Cast;
import org.elasticsearch.painless.Definition.def;

import java.util.Objects;

/**
 * Used during the analysis phase to collect legal type casts and promotions
 * for type-checking and later to write necessary casts in the bytecode.
 */
public final class AnalyzerCaster {

    public static Cast getLegalCast(Location location, Class<?> actual, Class<?> expected, boolean explicit, boolean internal) {
        Objects.requireNonNull(actual);
        Objects.requireNonNull(expected);

        if (actual == expected) {
            return null;
        }

        if (actual == def.class) {
            if (expected == boolean.class) {
                return Cast.unboxTo(def.class, Boolean.class, explicit, boolean.class);
            } else if (expected == byte.class) {
                return Cast.unboxTo(def.class, Byte.class, explicit, byte.class);
            } else if (expected == short.class) {
                return Cast.unboxTo(def.class, Short.class, explicit, short.class);
            } else if (expected == char.class) {
                return Cast.unboxTo(def.class, Character.class, explicit, char.class);
            } else if (expected == int.class) {
                return Cast.unboxTo(def.class, Integer.class, explicit, int.class);
            } else if (expected == long.class) {
                return Cast.unboxTo(def.class, Long.class, explicit, long.class);
            } else if (expected == float.class) {
                return Cast.unboxTo(def.class, Float.class, explicit, float.class);
            } else if (expected == double.class) {
                return Cast.unboxTo(def.class, Double.class, explicit, double.class);
            }
        } else if (actual == Object.class) {
            if (expected == byte.class && explicit && internal) {
                return Cast.unboxTo(Object.class, Byte.class, true, byte.class);
            } else if (expected == short.class && explicit && internal) {
                return Cast.unboxTo(Object.class, Short.class, true, short.class);
            } else if (expected == char.class && explicit && internal) {
                return Cast.unboxTo(Object.class, Character.class, true, char.class);
            } else if (expected == int.class && explicit && internal) {
                return Cast.unboxTo(Object.class, Integer.class, true, int.class);
            } else if (expected == long.class && explicit && internal) {
                return Cast.unboxTo(Object.class, Long.class, true, long.class);
            } else if (expected == float.class && explicit && internal) {
                return Cast.unboxTo(Object.class, Float.class, true, float.class);
            } else if (expected == double.class && explicit && internal) {
                return Cast.unboxTo(Object.class, Double.class, true, double.class);
            }
        } else if (actual == Number.class) {
            if (expected == byte.class && explicit && internal) {
                return Cast.unboxTo(Number.class, Byte.class, true, byte.class);
            } else if (expected == short.class && explicit && internal) {
                return Cast.unboxTo(Number.class, Short.class, true, short.class);
            } else if (expected == char.class && explicit && internal) {
                return Cast.unboxTo(Number.class, Character.class, true, char.class);
            } else if (expected == int.class && explicit && internal) {
                return Cast.unboxTo(Number.class, Integer.class, true, int.class);
            } else if (expected == long.class && explicit && internal) {
                return Cast.unboxTo(Number.class, Long.class, true, long.class);
            } else if (expected == float.class && explicit && internal) {
                return Cast.unboxTo(Number.class, Float.class, true, float.class);
            } else if (expected == double.class && explicit && internal) {
                return Cast.unboxTo(Number.class, Double.class, true, double.class);
            }
        } else if (actual == String.class) {
            if (expected == char.class && explicit) {
                return Cast.standard(String.class, char.class, true);
            }
        } else if (actual == boolean.class) {
            if (expected == def.class) {
                return Cast.boxFrom(Boolean.class, def.class, explicit, boolean.class);
            } else if (expected == Object.class && internal) {
                return Cast.boxFrom(Boolean.class, Object.class, explicit, boolean.class);
            } else if (expected == Boolean.class && internal) {
                return Cast.boxTo(boolean.class, boolean.class, explicit, boolean.class);
            }
        } else if (actual == byte.class) {
            if (expected == def.class) {
                return Cast.boxFrom(Byte.class, def.class, explicit, byte.class);
            } else if (expected == Object.class && internal) {
                return Cast.boxFrom(Byte.class, Object.class, explicit, byte.class);
            } else if (expected == Number.class && internal) {
                return Cast.boxFrom(Byte.class, Number.class, explicit, byte.class);
            } else if (expected == short.class) {
                return Cast.standard(byte.class, short.class, explicit);
            } else if (expected == char.class && explicit) {
                return Cast.standard(byte.class, char.class, true);
            } else if (expected == int.class) {
                return Cast.standard(byte.class, int.class, explicit);
            } else if (expected == long.class) {
                return Cast.standard(byte.class, long.class, explicit);
            } else if (expected == float.class) {
                return Cast.standard(byte.class, float.class, explicit);
            } else if (expected == double.class) {
                return Cast.standard(byte.class, double.class, explicit);
            } else if (expected == Byte.class && internal) {
                return Cast.boxTo(byte.class, byte.class, explicit, byte.class);
            } else if (expected == Short.class && internal) {
                return Cast.boxTo(byte.class, short.class, explicit, short.class);
            } else if (expected == Character.class && explicit && internal) {
                return Cast.boxTo(byte.class, char.class, true, char.class);
            } else if (expected == Integer.class && internal) {
                return Cast.boxTo(byte.class, int.class, explicit, int.class);
            } else if (expected == Long.class && internal) {
                return Cast.boxTo(byte.class, long.class, explicit, long.class);
            } else if (expected == Float.class && internal) {
                return Cast.boxTo(byte.class, float.class, explicit, float.class);
            } else if (expected == Double.class && internal) {
                return Cast.boxTo(byte.class, double.class, explicit, double.class);
            }
        } else if (actual == short.class) {
            if (expected == def.class) {
                return Cast.boxFrom(Short.class, def.class, explicit, short.class);
            } else if (expected == Object.class && internal) {
                return Cast.boxFrom(Short.class, Object.class, explicit, short.class);
            } else if (expected == Number.class && internal) {
                return Cast.boxFrom(Short.class, Number.class, explicit, short.class);
            } else if (expected == byte.class && explicit) {
                return Cast.standard(short.class, byte.class, true);
            } else if (expected == char.class && explicit) {
                return Cast.standard(short.class, char.class, true);
            } else if (expected == int.class) {
                return Cast.standard(short.class, int.class, explicit);
            } else if (expected == long.class) {
                return Cast.standard(short.class, long.class, explicit);
            } else if (expected == float.class) {
                return Cast.standard(short.class, float.class, explicit);
            } else if (expected == double.class) {
                return Cast.standard(short.class, double.class, explicit);
            } else if (expected == Byte.class && explicit && internal) {
                return Cast.boxTo(short.class, byte.class, true, byte.class);
            } else if (expected == Short.class && internal) {
                return Cast.boxTo(short.class, short.class, explicit, short.class);
            } else if (expected == Character.class && explicit && internal) {
                return Cast.boxTo(short.class, char.class, true, char.class);
            } else if (expected == Integer.class && internal) {
                return Cast.boxTo(short.class, int.class, explicit, int.class);
            } else if (expected == Long.class && internal) {
                return Cast.boxTo(short.class, long.class, explicit, long.class);
            } else if (expected == Float.class && internal) {
                return Cast.boxTo(short.class, float.class, explicit, float.class);
            } else if (expected == Double.class && internal) {
                return Cast.boxTo(short.class, double.class, explicit, double.class);
            }
        } else if (actual == char.class) {
            if (expected == def.class) {
                return Cast.boxFrom(Character.class, def.class, explicit, char.class);
            } else if (expected == Object.class && internal) {
                return Cast.boxFrom(Character.class, Object.class, explicit, char.class);
            } else if (expected == Number.class && internal) {
                return Cast.boxFrom(Character.class, Number.class, explicit, char.class);
            } else if (expected == String.class) {
                return Cast.standard(char.class, String.class, explicit);
            } else if (expected == byte.class && explicit) {
                return Cast.standard(char.class, byte.class, true);
            } else if (expected == short.class && explicit) {
                return Cast.standard(char.class, short.class, true);
            } else if (expected == int.class) {
                return Cast.standard(char.class, int.class, explicit);
            } else if (expected == long.class) {
                return Cast.standard(char.class, long.class, explicit);
            } else if (expected == float.class) {
                return Cast.standard(char.class, float.class, explicit);
            } else if (expected == double.class) {
                return Cast.standard(char.class, double.class, explicit);
            } else if (expected == Byte.class && explicit && internal) {
                return Cast.boxTo(char.class, byte.class, true, byte.class);
            } else if (expected == Short.class && internal) {
                return Cast.boxTo(char.class, short.class, explicit, short.class);
            } else if (expected == Character.class && internal) {
                return Cast.boxTo(char.class, char.class, true, char.class);
            } else if (expected == Integer.class && internal) {
                return Cast.boxTo(char.class, int.class, explicit, int.class);
            } else if (expected == Long.class && internal) {
                return Cast.boxTo(char.class, long.class, explicit, long.class);
            } else if (expected == Float.class && internal) {
                return Cast.boxTo(char.class, float.class, explicit, float.class);
            } else if (expected == Double.class && internal) {
                return Cast.boxTo(char.class, double.class, explicit, double.class);
            }
        } else if (actual == int.class) {
            if (expected == def.class) {
                return Cast.boxFrom(Integer.class, def.class, explicit, int.class);
            } else if (expected == Object.class && internal) {
                return Cast.boxFrom(Integer.class, Object.class, explicit, int.class);
            } else if (expected == Number.class && internal) {
                return Cast.boxFrom(Integer.class, Number.class, explicit, int.class);
            } else if (expected == byte.class && explicit) {
                return Cast.standard(int.class, byte.class, true);
            } else if (expected == char.class && explicit) {
                return Cast.standard(int.class, char.class, true);
            } else if (expected == short.class && explicit) {
                return Cast.standard(int.class, short.class, true);
            } else if (expected == long.class) {
                return Cast.standard(int.class, long.class, explicit);
            } else if (expected == float.class) {
                return Cast.standard(int.class, float.class, explicit);
            } else if (expected == double.class) {
                return Cast.standard(int.class, double.class, explicit);
            } else if (expected == Byte.class && explicit && internal) {
                return Cast.boxTo(int.class, byte.class, true, byte.class);
            } else if (expected == Short.class && explicit && internal) {
                return Cast.boxTo(int.class, short.class, true, short.class);
            } else if (expected == Character.class && explicit && internal) {
                return Cast.boxTo(int.class, char.class, true, char.class);
            } else if (expected == Integer.class && internal) {
                return Cast.boxTo(int.class, int.class, explicit, int.class);
            } else if (expected == Long.class && internal) {
                return Cast.boxTo(int.class, long.class, explicit, long.class);
            } else if (expected == Float.class && internal) {
                return Cast.boxTo(int.class, float.class, explicit, float.class);
            } else if (expected == Double.class && internal) {
                return Cast.boxTo(int.class, double.class, explicit, double.class);
            }
        } else if (actual == long.class) {
            if (expected == def.class) {
                return Cast.boxFrom(Long.class, def.class, explicit, long.class);
            } else if (expected == Object.class && internal) {
                return Cast.boxFrom(Long.class, Object.class, explicit, long.class);
            } else if (expected == Number.class && internal) {
                return Cast.boxFrom(Long.class, Number.class, explicit, long.class);
            } else if (expected == byte.class && explicit) {
                return Cast.standard(long.class, byte.class, true);
            } else if (expected == char.class && explicit) {
                return Cast.standard(long.class, char.class, true);
            } else if (expected == short.class && explicit) {
                return Cast.standard(long.class, short.class, true);
            } else if (expected == int.class && explicit) {
                return Cast.standard(long.class, int.class, true);
            } else if (expected == float.class) {
                return Cast.standard(long.class, float.class, explicit);
            } else if (expected == double.class) {
                return Cast.standard(long.class, double.class, explicit);
            } else if (expected == Byte.class && explicit && internal) {
                return Cast.boxTo(long.class, byte.class, true, byte.class);
            } else if (expected == Short.class && explicit && internal) {
                return Cast.boxTo(long.class, short.class, true, short.class);
            } else if (expected == Character.class && explicit && internal) {
                return Cast.boxTo(long.class, char.class, true, char.class);
            } else if (expected == Integer.class && explicit && internal) {
                return Cast.boxTo(long.class, int.class, true, int.class);
            } else if (expected == Long.class && internal) {
                return Cast.boxTo(long.class, long.class, explicit, long.class);
            } else if (expected == Float.class && internal) {
                return Cast.boxTo(long.class, float.class, explicit, float.class);
            } else if (expected == Double.class && internal) {
                return Cast.boxTo(long.class, double.class, explicit, double.class);
            }
        } else if (actual == float.class) {
            if (expected == def.class) {
                return Cast.boxFrom(Float.class, def.class, explicit, float.class);
            } else if (expected == Object.class && internal) {
                return Cast.boxFrom(Float.class, Object.class, explicit, float.class);
            } else if (expected == Number.class && internal) {
                return Cast.boxFrom(Float.class, Number.class, explicit, float.class);
            } else if (expected == byte.class && explicit) {
                return Cast.standard(float.class, byte.class, true);
            } else if (expected == char.class && explicit) {
                return Cast.standard(float.class, char.class, true);
            } else if (expected == short.class && explicit) {
                return Cast.standard(float.class, short.class, true);
            } else if (expected == int.class && explicit) {
                return Cast.standard(float.class, int.class, true);
            } else if (expected == long.class && explicit) {
                return Cast.standard(float.class, long.class, true);
            } else if (expected == double.class) {
                return Cast.standard(float.class, double.class, explicit);
            } else if (expected == Byte.class && explicit && internal) {
                return Cast.boxTo(float.class, byte.class, true, byte.class);
            } else if (expected == Short.class && explicit && internal) {
                return Cast.boxTo(float.class, short.class, true, short.class);
            } else if (expected == Character.class && explicit && internal) {
                return Cast.boxTo(float.class, char.class, true, char.class);
            } else if (expected == Integer.class && explicit && internal) {
                return Cast.boxTo(float.class, int.class, true, int.class);
            } else if (expected == Long.class && explicit && internal) {
                return Cast.boxTo(float.class, long.class, true, long.class);
            } else if (expected == Float.class && internal) {
                return Cast.boxTo(float.class, float.class, explicit, float.class);
            } else if (expected == Double.class && internal) {
                return Cast.boxTo(float.class, double.class, explicit, double.class);
            }
        } else if (actual == double.class) {
            if (expected == def.class) {
                return Cast.boxFrom(Double.class, def.class, explicit, double.class);
            } else if (expected == Object.class && internal) {
                return Cast.boxFrom(Double.class, Object.class, explicit, double.class);
            } else if (expected == Number.class && internal) {
                return Cast.boxFrom(Double.class, Number.class, explicit, double.class);
            } else if (expected == byte.class && explicit) {
                return Cast.standard(double.class, byte.class, true);
            } else if (expected == char.class && explicit) {
                return Cast.standard(double.class, char.class, true);
            } else if (expected == short.class && explicit) {
                return Cast.standard(double.class, short.class, true);
            } else if (expected == int.class && explicit) {
                return Cast.standard(double.class, int.class, true);
            } else if (expected == long.class && explicit) {
                return Cast.standard(double.class, long.class, true);
            } else if (expected == float.class && explicit) {
                return Cast.standard(double.class, float.class, true);
            } else if (expected == Byte.class && explicit && internal) {
                return Cast.boxTo(double.class, byte.class, true, byte.class);
            } else if (expected == Short.class && explicit && internal) {
                return Cast.boxTo(double.class, short.class, true, short.class);
            } else if (expected == Character.class && explicit && internal) {
                return Cast.boxTo(double.class, char.class, true, char.class);
            } else if (expected == Integer.class && explicit && internal) {
                return Cast.boxTo(double.class, int.class, true, int.class);
            } else if (expected == Long.class && explicit && internal) {
                return Cast.boxTo(double.class, long.class, true, long.class);
            } else if (expected == Float.class && explicit && internal) {
                return Cast.boxTo(double.class, float.class, true, float.class);
            } else if (expected == Double.class && internal) {
                return Cast.boxTo(double.class, double.class, explicit, double.class);
            }
        } else if (actual == Boolean.class) {
            if (expected == boolean.class && internal) {
                return Cast.unboxFrom(boolean.class, boolean.class, explicit, boolean.class);
            }
        } else if (actual == Byte.class) {
            if (expected == byte.class && internal) {
                return Cast.unboxFrom(byte.class, byte.class, explicit, byte.class);
            } else if (expected == short.class && internal) {
                return Cast.unboxFrom(byte.class, short.class, explicit, byte.class);
            } else if (expected == char.class && explicit && internal) {
                return Cast.unboxFrom(byte.class, char.class, true, byte.class);
            } else if (expected == int.class && internal) {
                return Cast.unboxFrom(byte.class, int.class, explicit, byte.class);
            } else if (expected == long.class && internal) {
                return Cast.unboxFrom(byte.class, long.class, explicit, byte.class);
            } else if (expected == float.class && internal) {
                return Cast.unboxFrom(byte.class, float.class, explicit, byte.class);
            } else if (expected == double.class && internal) {
                return Cast.unboxFrom(byte.class, double.class, explicit, byte.class);
            }
        } else if (actual == Short.class) {
            if (expected == byte.class && explicit && internal) {
                return Cast.unboxFrom(short.class, byte.class, true, short.class);
            } else if (expected == short.class && internal) {
                return Cast.unboxFrom(short.class, short.class, explicit, short.class);
            } else if (expected == char.class && explicit && internal) {
                return Cast.unboxFrom(short.class, char.class, true, short.class);
            } else if (expected == int.class && internal) {
                return Cast.unboxFrom(short.class, int.class, explicit, short.class);
            } else if (expected == long.class && internal) {
                return Cast.unboxFrom(short.class, long.class, explicit, short.class);
            } else if (expected == float.class && internal) {
                return Cast.unboxFrom(short.class, float.class, explicit, short.class);
            } else if (expected == double.class && internal) {
                return Cast.unboxFrom(short.class, double.class, explicit, short.class);
            }
        } else if (actual == Character.class) {
            if (expected == byte.class && explicit && internal) {
                return Cast.unboxFrom(char.class, byte.class, true, char.class);
            } else if (expected == short.class && explicit && internal) {
                return Cast.unboxFrom(char.class, short.class, true, char.class);
            } else if (expected == char.class && internal) {
                return Cast.unboxFrom(char.class, char.class, explicit, char.class);
            } else if (expected == int.class && internal) {
                return Cast.unboxFrom(char.class, int.class, explicit, char.class);
            } else if (expected == long.class && internal) {
                return Cast.unboxFrom(char.class, long.class, explicit, char.class);
            } else if (expected == float.class && internal) {
                return Cast.unboxFrom(char.class, float.class, explicit, char.class);
            } else if (expected == double.class && internal) {
                return Cast.unboxFrom(char.class, double.class, explicit, char.class);
            }
        } else if (actual == Integer.class) {
            if (expected == byte.class && explicit && internal) {
                return Cast.unboxFrom(int.class, byte.class, true, int.class);
            } else if (expected == short.class && explicit && internal) {
                return Cast.unboxFrom(int.class, short.class, true, int.class);
            } else if (expected == char.class && explicit && internal) {
                return Cast.unboxFrom(int.class, char.class, true, int.class);
            } else if (expected == int.class && internal) {
                return Cast.unboxFrom(int.class, int.class, explicit, int.class);
            } else if (expected == long.class && internal) {
                return Cast.unboxFrom(int.class, long.class, explicit, int.class);
            } else if (expected == float.class && internal) {
                return Cast.unboxFrom(int.class, float.class, explicit, int.class);
            } else if (expected == double.class && internal) {
                return Cast.unboxFrom(int.class, double.class, explicit, int.class);
            }
        } else if (actual == Long.class) {
            if (expected == byte.class && explicit && internal) {
                return Cast.unboxFrom(long.class, byte.class, true, long.class);
            } else if (expected == short.class && explicit && internal) {
                return Cast.unboxFrom(long.class, short.class, true, long.class);
            } else if (expected == char.class && explicit && internal) {
                return Cast.unboxFrom(long.class, char.class, true, long.class);
            } else if (expected == int.class && explicit && internal) {
                return Cast.unboxFrom(long.class, int.class, true, long.class);
            } else if (expected == long.class && internal) {
                return Cast.unboxFrom(long.class, long.class, explicit, long.class);
            } else if (expected == float.class && internal) {
                return Cast.unboxFrom(long.class, float.class, explicit, long.class);
            } else if (expected == double.class && internal) {
                return Cast.unboxFrom(long.class, double.class, explicit, long.class);
            }
        } else if (actual == Float.class) {
            if (expected == byte.class && explicit && internal) {
                return Cast.unboxFrom(float.class, byte.class, true, float.class);
            } else if (expected == short.class && explicit && internal) {
                return Cast.unboxFrom(float.class, short.class, true, float.class);
            } else if (expected == char.class && explicit && internal) {
                return Cast.unboxFrom(float.class, char.class, true, float.class);
            } else if (expected == int.class && explicit && internal) {
                return Cast.unboxFrom(float.class, int.class, true, float.class);
            } else if (expected == long.class && explicit && internal) {
                return Cast.unboxFrom(float.class, long.class, true, float.class);
            } else if (expected == float.class && internal) {
                return Cast.unboxFrom(float.class, float.class, explicit, float.class);
            } else if (expected == double.class && internal) {
                return Cast.unboxFrom(float.class, double.class, explicit, float.class);
            }
        } else if (actual == Double.class) {
            if (expected == byte.class && explicit && internal) {
                return Cast.unboxFrom(double.class, byte.class, true, double.class);
            } else if (expected == short.class && explicit && internal) {
                return Cast.unboxFrom(double.class, short.class, true, double.class);
            } else if (expected == char.class && explicit && internal) {
                return Cast.unboxFrom(double.class, char.class, true, double.class);
            } else if (expected == int.class && explicit && internal) {
                return Cast.unboxFrom(double.class, int.class, true, double.class);
            } else if (expected == long.class && explicit && internal) {
                return Cast.unboxFrom(double.class, long.class, true, double.class);
            } else if (expected == float.class && explicit && internal) {
                return Cast.unboxFrom(double.class, float.class, true, double.class);
            } else if (expected == double.class && internal) {
                return Cast.unboxFrom(double.class, double.class, explicit, double.class);
            }
        }

        if (    actual == def.class                                   ||
                (actual != void.class && expected == def.class) ||
                expected.isAssignableFrom(actual)    ||
                (actual.isAssignableFrom(expected) && explicit)) {
            return Cast.standard(actual, expected, explicit);
        } else {
            throw location.createError(new ClassCastException(
                    "Cannot cast from [" + Definition.ClassToName(actual) + "] to [" + Definition.ClassToName(expected) + "]."));
        }
    }

    public static Object constCast(Location location, Object constant, Cast cast) {
        Class<?> fsort = cast.from;
        Class<?> tsort = cast.to;

        if (fsort == tsort) {
            return constant;
        } else if (fsort == String.class && tsort == char.class) {
            return Utility.StringTochar((String)constant);
        } else if (fsort == char.class && tsort == String.class) {
            return Utility.charToString((char)constant);
        } else if (fsort.isPrimitive() && fsort != boolean.class && tsort.isPrimitive() && tsort != boolean.class) {
            Number number;

            if (fsort == char.class) {
                number = (int)(char)constant;
            } else {
                number = (Number)constant;
            }

            if      (tsort == byte.class) return number.byteValue();
            else if (tsort == short.class) return number.shortValue();
            else if (tsort == char.class) return (char)number.intValue();
            else if (tsort == int.class) return number.intValue();
            else if (tsort == long.class) return number.longValue();
            else if (tsort == float.class) return number.floatValue();
            else if (tsort == double.class) return number.doubleValue();
            else {
                throw location.createError(new IllegalStateException("Cannot cast from " +
                    "[" + cast.from.getCanonicalName() + "] to [" + cast.to.getCanonicalName() + "]."));
            }
        } else {
            throw location.createError(new IllegalStateException("Cannot cast from " +
                "[" + cast.from.getCanonicalName() + "] to [" + cast.to.getCanonicalName() + "]."));
        }
    }

    public static Class<?> promoteNumeric(Class<?> from, boolean decimal) {
        if (from == def.class || from == double.class && decimal || from == float.class && decimal || from == long.class) {
            return from;
        } else if (from == int.class || from == char.class || from == short.class || from == byte.class) {
            return int.class;
        }

        return null;
    }

    public static Class<?> promoteNumeric(Class<?> from0, Class<?> from1, boolean decimal) {
        if (from0 == def.class || from1 == def.class) {
            return def.class;
        }

        if (decimal) {
            if (from0 == double.class || from1 == double.class) {
                return double.class;
            } else if (from0 == float.class || from1 == float.class) {
                return float.class;
            }
        }

        if (from0 == long.class || from1 == long.class) {
            return long.class;
        } else if (from0 == int.class   || from1 == int.class   ||
                   from0 == char.class  || from1 == char.class  ||
                   from0 == short.class || from1 == short.class ||
                   from0 == byte.class  || from1 == byte.class) {
            return int.class;
        }

        return null;
    }

    public static Class<?> promoteAdd(Class<?> from0, Class<?> from1) {
        if (from0 == String.class || from1 == String.class) {
            return String.class;
        }

        return promoteNumeric(from0, from1, true);
    }

    public static Class<?> promoteXor(Class<?> from0, Class<?> from1) {
        if (from0 == def.class || from1 == def.class) {
            return def.class;
        }

        if (from0 == boolean.class || from1 == boolean.class) {
            return boolean.class;
        }

        return promoteNumeric(from0, from1, false);
    }

    public static Class<?> promoteEquality(Class<?> from0, Class<?> from1) {
        if (from0 == def.class || from1 == def.class) {
            return def.class;
        }

        if (from0.isPrimitive() && from1.isPrimitive()) {
            if (from0 == boolean.class && from1 == boolean.class) {
                return boolean.class;
            }

            return promoteNumeric(from0, from1, true);
        }

        return Object.class;
    }

    public static Class<?> promoteConditional(Class<?> from0, Class<?> from1, Object const0, Object const1) {
        if (from0 == from1) {
            return from0;
        }

        if (from0 == def.class || from1 == def.class) {
            return def.class;
        }

        if (from0.isPrimitive() && from1.isPrimitive()) {
            if (from0 == boolean.class && from1 == boolean.class) {
                return boolean.class;
            }

            if (from0 == double.class || from1 == double.class) {
                return double.class;
            } else if (from0 == float.class || from1 == float.class) {
                return float.class;
            } else if (from0 == long.class || from1 == long.class) {
                return long.class;
            } else {
                if (from0 == byte.class) {
                    if (from1 == byte.class) {
                        return byte.class;
                    } else if (from1 == short.class) {
                        if (const1 != null) {
                            final short constant = (short)const1;

                            if (constant <= Byte.MAX_VALUE && constant >= Byte.MIN_VALUE) {
                                return byte.class;
                            }
                        }

                        return short.class;
                    } else if (from1 == char.class) {
                        return int.class;
                    } else if (from1 == int.class) {
                        if (const1 != null) {
                            final int constant = (int)const1;

                            if (constant <= Byte.MAX_VALUE && constant >= Byte.MIN_VALUE) {
                                return byte.class;
                            }
                        }

                        return int.class;
                    }
                } else if (from0 == short.class) {
                    if (from1 == byte.class) {
                        if (const0 != null) {
                            final short constant = (short)const0;

                            if (constant <= Byte.MAX_VALUE && constant >= Byte.MIN_VALUE) {
                                return byte.class;
                            }
                        }

                        return short.class;
                    } else if (from1 == short.class) {
                        return short.class;
                    } else if (from1 == char.class) {
                        return int.class;
                    } else if (from1 == int.class) {
                        if (const1 != null) {
                            final int constant = (int)const1;

                            if (constant <= Short.MAX_VALUE && constant >= Short.MIN_VALUE) {
                                return short.class;
                            }
                        }

                        return int.class;
                    }
                } else if (from0 == char.class) {
                    if (from1 == byte.class) {
                        return int.class;
                    } else if (from1 == short.class) {
                        return int.class;
                    } else if (from1 == char.class) {
                        return char.class;
                    } else if (from1 == int.class) {
                        if (const1 != null) {
                            final int constant = (int)const1;

                            if (constant <= Character.MAX_VALUE && constant >= Character.MIN_VALUE) {
                                return byte.class;
                            }
                        }

                        return int.class;
                    }
                } else if (from0 == int.class) {
                    if (from1 == byte.class) {
                        if (const0 != null) {
                            final int constant = (int)const0;

                            if (constant <= Byte.MAX_VALUE && constant >= Byte.MIN_VALUE) {
                                return byte.class;
                            }
                        }

                        return int.class;
                    } else if (from1 == short.class) {
                        if (const0 != null) {
                            final int constant = (int)const0;

                            if (constant <= Short.MAX_VALUE && constant >= Short.MIN_VALUE) {
                                return byte.class;
                            }
                        }

                        return int.class;
                    } else if (from1 == char.class) {
                        if (const0 != null) {
                            final int constant = (int)const0;

                            if (constant <= Character.MAX_VALUE && constant >= Character.MIN_VALUE) {
                                return byte.class;
                            }
                        }

                        return int.class;
                    } else if (from1 == int.class) {
                        return int.class;
                    }
                }
            }
        }

        // TODO: In the rare case we still haven't reached a correct promotion we need
        // TODO: to calculate the highest upper bound for the two types and return that.
        // TODO: However, for now we just return objectType that may require an extra cast.

        return Object.class;
    }

    private AnalyzerCaster() {

    }
}
