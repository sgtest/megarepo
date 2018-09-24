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

package org.elasticsearch.client.watcher;

import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.common.joda.FormatDateTimeFormatter;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.mapper.DateFieldMapper;
import org.joda.time.DateTime;
import org.joda.time.DateTimeZone;

import java.io.IOException;

public final class WatchStatusDateParser {

    private static final FormatDateTimeFormatter FORMATTER = DateFieldMapper.DEFAULT_DATE_TIME_FORMATTER;

    private WatchStatusDateParser() {
        // Prevent instantiation.
    }

    public static DateTime parseDate(String fieldName, XContentParser parser) throws IOException {
        XContentParser.Token token = parser.currentToken();
        if (token == XContentParser.Token.VALUE_NUMBER) {
            return new DateTime(parser.longValue(), DateTimeZone.UTC);
        }
        if (token == XContentParser.Token.VALUE_STRING) {
            DateTime dateTime = parseDate(parser.text());
            return dateTime.toDateTime(DateTimeZone.UTC);
        }
        if (token == XContentParser.Token.VALUE_NULL) {
            return null;
        }
        throw new ElasticsearchParseException("could not parse date/time. expected date field [{}] " +
            "to be either a number or a string but found [{}] instead", fieldName, token);
    }

    public static DateTime parseDate(String text) {
        return FORMATTER.parser().parseDateTime(text);
    }
}
