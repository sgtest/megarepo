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

package org.elasticsearch.http.nio.cors;

import io.netty.handler.codec.http.HttpMethod;

import java.util.Arrays;
import java.util.Date;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.Callable;
import java.util.regex.Pattern;

/**
 * Builder used to configure and build a {@link NioCorsConfig} instance.
 *
 * This class was lifted from the Netty project:
 *  https://github.com/netty/netty
 */
public final class NioCorsConfigBuilder {

    /**
     * Creates a Builder instance with it's origin set to '*'.
     *
     * @return Builder to support method chaining.
     */
    public static NioCorsConfigBuilder forAnyOrigin() {
        return new NioCorsConfigBuilder();
    }

    /**
     * Create a {@link NioCorsConfigBuilder} instance with the specified pattern origin.
     *
     * @param pattern the regular expression pattern to match incoming origins on.
     * @return {@link NioCorsConfigBuilder} with the configured origin pattern.
     */
    public static NioCorsConfigBuilder forPattern(final Pattern pattern) {
        if (pattern == null) {
            throw new IllegalArgumentException("CORS pattern cannot be null");
        }
        return new NioCorsConfigBuilder(pattern);
    }

    /**
     * Creates a {@link NioCorsConfigBuilder} instance with the specified origins.
     *
     * @return {@link NioCorsConfigBuilder} to support method chaining.
     */
    public static NioCorsConfigBuilder forOrigins(final String... origins) {
        return new NioCorsConfigBuilder(origins);
    }

    Optional<Set<String>> origins;
    Optional<Pattern> pattern;
    final boolean anyOrigin;
    boolean enabled = true;
    boolean allowCredentials;
    long maxAge;
    final Set<HttpMethod> requestMethods = new HashSet<>();
    final Set<String> requestHeaders = new HashSet<>();
    final Map<CharSequence, Callable<?>> preflightHeaders = new HashMap<>();
    boolean shortCircuit;

    /**
     * Creates a new Builder instance with the origin passed in.
     *
     * @param origins the origin to be used for this builder.
     */
    NioCorsConfigBuilder(final String... origins) {
        this.origins = Optional.of(new LinkedHashSet<>(Arrays.asList(origins)));
        pattern = Optional.empty();
        anyOrigin = false;
    }

    /**
     * Creates a new Builder instance allowing any origin, "*" which is the
     * wildcard origin.
     *
     */
    NioCorsConfigBuilder() {
        anyOrigin = true;
        origins = Optional.empty();
        pattern = Optional.empty();
    }

    /**
     * Creates a new Builder instance allowing any origin that matches the pattern.
     *
     * @param pattern the pattern to match against for incoming origins.
     */
    NioCorsConfigBuilder(final Pattern pattern) {
        this.pattern = Optional.of(pattern);
        origins = Optional.empty();
        anyOrigin = false;
    }

    /**
     * Disables CORS support.
     *
     * @return {@link NioCorsConfigBuilder} to support method chaining.
     */
    public NioCorsConfigBuilder disable() {
        enabled = false;
        return this;
    }

    /**
     * By default cookies are not included in CORS requests, but this method will enable cookies to
     * be added to CORS requests. Calling this method will set the CORS 'Access-Control-Allow-Credentials'
     * response header to true.
     *
     * Please note, that cookie support needs to be enabled on the client side as well.
     * The client needs to opt-in to send cookies by calling:
     * <pre>
     * xhr.withCredentials = true;
     * </pre>
     * The default value for 'withCredentials' is false in which case no cookies are sent.
     * Setting this to true will included cookies in cross origin requests.
     *
     * @return {@link NioCorsConfigBuilder} to support method chaining.
     */
    public NioCorsConfigBuilder allowCredentials() {
        allowCredentials = true;
        return this;
    }

    /**
     * When making a preflight request the client has to perform two request with can be inefficient.
     * This setting will set the CORS 'Access-Control-Max-Age' response header and enables the
     * caching of the preflight response for the specified time. During this time no preflight
     * request will be made.
     *
     * @param max the maximum time, in seconds, that the preflight response may be cached.
     * @return {@link NioCorsConfigBuilder} to support method chaining.
     */
    public NioCorsConfigBuilder maxAge(final long max) {
        maxAge = max;
        return this;
    }

    /**
     * Specifies the allowed set of HTTP Request Methods that should be returned in the
     * CORS 'Access-Control-Request-Method' response header.
     *
     * @param methods the {@link HttpMethod}s that should be allowed.
     * @return {@link NioCorsConfigBuilder} to support method chaining.
     */
    public NioCorsConfigBuilder allowedRequestMethods(final HttpMethod... methods) {
        requestMethods.addAll(Arrays.asList(methods));
        return this;
    }

    /**
     * Specifies the if headers that should be returned in the CORS 'Access-Control-Allow-Headers'
     * response header.
     *
     * If a client specifies headers on the request, for example by calling:
     * <pre>
     * xhr.setRequestHeader('My-Custom-Header', "SomeValue");
     * </pre>
     * the server will receive the above header name in the 'Access-Control-Request-Headers' of the
     * preflight request. The server will then decide if it allows this header to be sent for the
     * real request (remember that a preflight is not the real request but a request asking the server
     * if it allow a request).
     *
     * @param headers the headers to be added to the preflight 'Access-Control-Allow-Headers' response header.
     * @return {@link NioCorsConfigBuilder} to support method chaining.
     */
    public NioCorsConfigBuilder allowedRequestHeaders(final String... headers) {
        requestHeaders.addAll(Arrays.asList(headers));
        return this;
    }

    /**
     * Specifies that a CORS request should be rejected if it's invalid before being
     * further processing.
     *
     * CORS headers are set after a request is processed. This may not always be desired
     * and this setting will check that the Origin is valid and if it is not valid no
     * further processing will take place, and a error will be returned to the calling client.
     *
     * @return {@link NioCorsConfigBuilder} to support method chaining.
     */
    public NioCorsConfigBuilder shortCircuit() {
        shortCircuit = true;
        return this;
    }

    /**
     * Builds a {@link NioCorsConfig} with settings specified by previous method calls.
     *
     * @return {@link NioCorsConfig} the configured CorsConfig instance.
     */
    public NioCorsConfig build() {
        if (preflightHeaders.isEmpty()) {
            preflightHeaders.put("date", DateValueGenerator.INSTANCE);
            preflightHeaders.put("content-length", new ConstantValueGenerator("0"));
        }
        return new NioCorsConfig(this);
    }

    /**
     * This class is used for preflight HTTP response values that do not need to be
     * generated, but instead the value is "static" in that the same value will be returned
     * for each call.
     */
    private static final class ConstantValueGenerator implements Callable<Object> {

        private final Object value;

        /**
         * Sole constructor.
         *
         * @param value the value that will be returned when the call method is invoked.
         */
        private ConstantValueGenerator(final Object value) {
            if (value == null) {
                throw new IllegalArgumentException("value must not be null");
            }
            this.value = value;
        }

        @Override
        public Object call() {
            return value;
        }
    }

    /**
     * This callable is used for the DATE preflight HTTP response HTTP header.
     * It's value must be generated when the response is generated, hence will be
     * different for every call.
     */
    private static final class DateValueGenerator implements Callable<Date> {

        static final DateValueGenerator INSTANCE = new DateValueGenerator();

        @Override
        public Date call() throws Exception {
            return new Date();
        }
    }

}
