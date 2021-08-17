/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.script;

import java.math.BigDecimal;
import java.math.BigInteger;

import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.List;
import java.util.stream.Collectors;

import static org.elasticsearch.script.Field.BigIntegerField;
import static org.elasticsearch.script.Field.BooleanField;
import static org.elasticsearch.script.Field.DoubleField;
import static org.elasticsearch.script.Field.DateMillisField;
import static org.elasticsearch.script.Field.DateNanosField;
import static org.elasticsearch.script.Field.LongField;
import static org.elasticsearch.script.Field.StringField;

/**
 * {@link Converters} for scripting fields.  These constants are exposed as static fields on {@link Field} to
 * allow a user to convert via {@link Field#as(Converter)}.
 */
public class Converters {
    /**
     * Convert to a {@link BigIntegerField} from Long, Double or String Fields.
     * Longs and Doubles are wrapped as BigIntegers.
     * Strings are parsed as either Longs or Doubles and wrapped in a BigInteger.
     */
    static final Converter<BigInteger, BigIntegerField> BIGINTEGER;

    /**
     * Convert to a {@link LongField} from Double, String, DateMillis, DateNanos, BigInteger or Boolean Fields.
     * Double is cast to a Long.
     * String is parsed as a Long.
     * DateMillis is milliseconds since epoch.
     * DateNanos is nanoseconds since epoch.
     * {@link BigInteger#longValue()} is used for the BigInteger conversion.
     * Boolean is {@code 1L} if {@code true}, {@code 0L} if {@code false}.
     */
    static final Converter<Long, LongField> LONG;

    static {
        BIGINTEGER = new Converter<>() {
            @Override
            public BigIntegerField convert(Field<?> sourceField) {
                if (sourceField instanceof LongField) {
                    return LongToBigInteger((LongField) sourceField);
                }

                if (sourceField instanceof DoubleField) {
                    return DoubleToBigInteger((DoubleField) sourceField);
                }

                if (sourceField instanceof StringField) {
                    return StringToBigInteger((StringField) sourceField);
                }

                if (sourceField instanceof DateMillisField) {
                    return LongToBigInteger(DateMillisToLong((DateMillisField) sourceField));
                }

                if (sourceField instanceof DateNanosField) {
                    return LongToBigInteger(DateNanosToLong((DateNanosField) sourceField));
                }

                if (sourceField instanceof BooleanField) {
                    return LongToBigInteger(BooleanToLong((BooleanField) sourceField));
                }

                throw new InvalidConversion(sourceField.getClass(), getFieldClass());
            }

            @Override
            public Class<BigIntegerField> getFieldClass() {
                return BigIntegerField.class;
            }

            @Override
            public Class<BigInteger> getTargetClass() {
                return BigInteger.class;
            }
        };

        LONG = new Converter<>() {
            @Override
            public LongField convert(Field<?> sourceField) {
                if (sourceField instanceof DoubleField) {
                    return DoubleToLong((DoubleField) sourceField);
                }

                if (sourceField instanceof StringField) {
                    return StringToLong((StringField) sourceField);
                }

                if (sourceField instanceof DateMillisField) {
                    return DateMillisToLong((DateMillisField) sourceField);
                }

                if (sourceField instanceof DateNanosField) {
                    return DateNanosToLong((DateNanosField) sourceField);
                }

                if (sourceField instanceof BigIntegerField) {
                    return BigIntegerToLong((BigIntegerField) sourceField);
                }

                if (sourceField instanceof BooleanField) {
                    return BooleanToLong((BooleanField) sourceField);
                }

                throw new InvalidConversion(sourceField.getClass(), getFieldClass());
            }

            @Override
            public Class<LongField> getFieldClass() {
                return LongField.class;
            }

            @Override
            public Class<Long> getTargetClass() {
                return Long.class;
            }
        };
    }

    // No instances, please
    private Converters() {}

    static BigIntegerField LongToBigInteger(LongField sourceField) {
        FieldValues<Long> fv = sourceField.getFieldValues();
        return new BigIntegerField(sourceField.getName(), new DelegatingFieldValues<>(fv) {
            @Override
            public List<BigInteger> getValues() {
                return values.getValues().stream().map(BigInteger::valueOf).collect(Collectors.toList());
            }

            @Override
            public BigInteger getNonPrimitiveValue() {
                return BigInteger.valueOf(values.getLongValue());
            }
        });
    }

    static BigIntegerField DoubleToBigInteger(DoubleField sourceField) {
        FieldValues<Double> fv = sourceField.getFieldValues();
        return new BigIntegerField(sourceField.getName(), new DelegatingFieldValues<>(fv) {
            @Override
            public List<BigInteger> getValues() {
                return values.getValues().stream().map(
                    dbl -> BigInteger.valueOf(dbl.longValue())
                ).collect(Collectors.toList());
            }

            @Override
            public BigInteger getNonPrimitiveValue() {
                return BigInteger.valueOf(values.getLongValue());
            }
        });
    }

    static BigIntegerField StringToBigInteger(StringField sourceField) {
        FieldValues<String> fv = sourceField.getFieldValues();
        return new BigIntegerField(sourceField.getName(), new DelegatingFieldValues<BigInteger, String>(fv) {
            protected BigInteger parseNumber(String str) {
                try {
                    return new BigInteger(str);
                } catch (NumberFormatException e) {
                    return new BigDecimal(str).toBigInteger();
                }
            }

            @Override
            public List<BigInteger> getValues() {
                // TODO(stu): this may throw
                return values.getValues().stream().map(this::parseNumber).collect(Collectors.toList());
            }

            @Override
            public BigInteger getNonPrimitiveValue() {
                return parseNumber(values.getNonPrimitiveValue());
            }
        });
    }

    static LongField BigIntegerToLong(BigIntegerField sourceField) {
        FieldValues<BigInteger> fv = sourceField.getFieldValues();
        return new LongField(sourceField.getName(), new DelegatingFieldValues<Long, BigInteger>(fv) {
            @Override
            public List<Long> getValues() {
                return values.getValues().stream().map(BigInteger::longValue).collect(Collectors.toList());
            }

            @Override
            public Long getNonPrimitiveValue() {
                return values.getLongValue();
            }
        });
    }

    static LongField BooleanToLong(BooleanField sourceField) {
        FieldValues<Boolean> fv = sourceField.getFieldValues();
        return new LongField(sourceField.getName(), new DelegatingFieldValues<Long, Boolean>(fv) {
            @Override
            public List<Long> getValues() {
                return values.getValues().stream().map(bool -> bool ? 1L : 0L).collect(Collectors.toList());
            }

            @Override
            public Long getNonPrimitiveValue() {
                return getLongValue();
            }
        });
    }

    static LongField DateMillisToLong(DateMillisField sourceField) {
        FieldValues<JodaCompatibleZonedDateTime> fv = sourceField.getFieldValues();
        return new LongField(sourceField.getName(), new DelegatingFieldValues<Long, JodaCompatibleZonedDateTime>(fv) {
            @Override
            public List<Long> getValues() {
                return values.getValues().stream().map(dt -> dt.toInstant().toEpochMilli()).collect(Collectors.toList());
            }

            @Override
            public Long getNonPrimitiveValue() {
                return values.getNonPrimitiveValue().toInstant().toEpochMilli();
            }
        });
    }

    static LongField DateNanosToLong(DateNanosField sourceField) {
        FieldValues<JodaCompatibleZonedDateTime> fv = sourceField.getFieldValues();
        return new LongField(sourceField.getName(), new DelegatingFieldValues<Long, JodaCompatibleZonedDateTime>(fv) {
            protected long nanoLong(JodaCompatibleZonedDateTime dt) {
                return ChronoUnit.NANOS.between(Instant.EPOCH, dt.toInstant());
            }

            @Override
            public List<Long> getValues() {
                return values.getValues().stream().map(this::nanoLong).collect(Collectors.toList());
            }

            @Override
            public Long getNonPrimitiveValue() {
                return ChronoUnit.NANOS.between(Instant.EPOCH, values.getNonPrimitiveValue().toInstant());
            }
        });
    }

    static LongField DoubleToLong(DoubleField sourceField) {
        FieldValues<Double> fv = sourceField.getFieldValues();
        return new LongField(sourceField.getName(), new DelegatingFieldValues<Long, Double>(fv) {
            @Override
            public List<Long> getValues() {
                return values.getValues().stream().map(Double::longValue).collect(Collectors.toList());
            }

            @Override
            public Long getNonPrimitiveValue() {
                return values.getLongValue();
            }
        });
    }

    static LongField StringToLong(StringField sourceField) {
        FieldValues<String> fv = sourceField.getFieldValues();
        return new LongField(sourceField.getName(), new DelegatingFieldValues<Long, String>(fv) {
            @Override
            public List<Long> getValues() {
                return values.getValues().stream().map(Long::parseLong).collect(Collectors.toList());
            }

            @Override
            public Long getNonPrimitiveValue() {
                return Long.parseLong(values.getNonPrimitiveValue());
            }

            @Override
            public long getLongValue() {
                return Long.parseLong(values.getNonPrimitiveValue());
            }

            @Override
            public double getDoubleValue() {
                return getLongValue();
            }
        });
    }

    /**
     * Helper for creating {@link Converter} classes which delegates all un-overridden methods to the underlying
     * {@link FieldValues}.
     */
    public abstract static class DelegatingFieldValues<T, D> implements FieldValues<T> {
        protected FieldValues<D> values;

        public DelegatingFieldValues(FieldValues<D> values) {
            this.values = values;
        }

        @Override
        public boolean isEmpty() {
            return values.isEmpty();
        }

        @Override
        public int size() {
            return values.size();
        }

        @Override
        public long getLongValue() {
            return values.getLongValue();
        }

        @Override
        public double getDoubleValue() {
            return values.getDoubleValue();
        }
    }
}
