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

package org.elasticsearch.search.aggregations.bucket.histogram;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.InstantiatingObjectParser;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContentFragment;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;

import java.io.IOException;
import java.util.Objects;

import static org.elasticsearch.common.xcontent.ConstructingObjectParser.optionalConstructorArg;

/**
 * Represent hard_bounds and extended_bounds in histogram aggregations.
 *
 * This class is similar to {@link LongBounds} used in date histograms, but is using longs to store data. LongBounds and DoubleBounds are
 * not used interchangeably and therefore don't share any common interfaces except for serialization.
 */

public class DoubleBounds implements ToXContentFragment, Writeable {
    static final ParseField MIN_FIELD = new ParseField("min");
    static final ParseField MAX_FIELD = new ParseField("max");
    static final InstantiatingObjectParser<DoubleBounds, Void> PARSER;

    static {
        InstantiatingObjectParser.Builder<DoubleBounds, Void> parser =
            InstantiatingObjectParser.builder("double_bounds", false, DoubleBounds.class);
        parser.declareField(optionalConstructorArg(), p -> p.currentToken() == XContentParser.Token.VALUE_NULL ? null : p.doubleValue(),
            MIN_FIELD, ObjectParser.ValueType.DOUBLE_OR_NULL);
        parser.declareField(optionalConstructorArg(), p -> p.currentToken() == XContentParser.Token.VALUE_NULL ? null : p.doubleValue(),
            MAX_FIELD, ObjectParser.ValueType.DOUBLE_OR_NULL);
        PARSER = parser.build();
    }

    /**
     * Min value
     */
    private final Double min;

    /**
     * Max value
     */
    private final Double max;

    /**
     * Construct with bounds.
     */
    public DoubleBounds(Double min, Double max) {
        this.min = min;
        this.max = max;
    }

    /**
     * Read from a stream.
     */
    public DoubleBounds(StreamInput in) throws IOException {
        min = in.readOptionalDouble();
        max = in.readOptionalDouble();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeOptionalDouble(min);
        out.writeOptionalDouble(max);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        if (min != null) {
            builder.field(MIN_FIELD.getPreferredName(), min);
        }
        if (max != null) {
            builder.field(MAX_FIELD.getPreferredName(), max);
        }
        return builder;
    }

    @Override
    public int hashCode() {
        return Objects.hash(min, max);
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == null) {
            return false;
        }
        if (getClass() != obj.getClass()) {
            return false;
        }
        DoubleBounds other = (DoubleBounds) obj;
        return Objects.equals(min, other.min)
                && Objects.equals(max, other.max);
    }

    public Double getMin() {
        return min;
    }

    public Double getMax() {
        return max;
    }

    public boolean contain(double value) {
        if (max != null && value > max) {
            return false;
        }
        if (min != null && value < min) {
            return false;
        }
        return true;
    }

    @Override
    public String toString() {
        StringBuilder b = new StringBuilder();
        if (min != null) {
            b.append(min);
        }
        b.append("--");
        if (max != null) {
            b.append(max);
        }
        return b.toString();
    }
}
