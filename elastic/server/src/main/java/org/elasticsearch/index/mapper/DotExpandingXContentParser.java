/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.elasticsearch.core.CheckedFunction;
import org.elasticsearch.xcontent.FilterXContentParser;
import org.elasticsearch.xcontent.FilterXContentParserWrapper;
import org.elasticsearch.xcontent.XContentLocation;
import org.elasticsearch.xcontent.XContentParser;
import org.elasticsearch.xcontent.XContentSubParser;

import java.io.IOException;
import java.util.ArrayDeque;
import java.util.Deque;
import java.util.List;
import java.util.Map;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;

/**
 * An XContentParser that reinterprets field names containing dots as an object structure.
 *
 * A field name named {@code "foo.bar.baz":...} will be parsed instead as {@code 'foo':{'bar':{'baz':...}}}.
 * The token location is preserved so that error messages refer to the original content being parsed.
 * This parser can output duplicate keys, but that is fine given that it's used for document parsing. The mapping
 * lookups will return the same mapper/field type, and we never load incoming documents in a map where duplicate
 * keys would end up overriding each other.
 */
class DotExpandingXContentParser extends FilterXContentParserWrapper {

    private static final class WrappingParser extends FilterXContentParser {

        private final BooleanSupplier isWithinLeafObject;
        final Deque<XContentParser> parsers = new ArrayDeque<>();

        WrappingParser(XContentParser in, BooleanSupplier isWithinLeafObject) throws IOException {
            this.isWithinLeafObject = isWithinLeafObject;
            parsers.push(in);
            if (in.currentToken() == Token.FIELD_NAME) {
                expandDots();
            }
        }

        @Override
        public Token nextToken() throws IOException {
            Token token;
            while ((token = delegate().nextToken()) == null) {
                parsers.pop();
                if (parsers.isEmpty()) {
                    return null;
                }
            }
            if (token != Token.FIELD_NAME) {
                return token;
            }
            expandDots();
            return Token.FIELD_NAME;
        }

        private void expandDots() throws IOException {
            // this handles fields that belong to objects that can't hold subobjects, where the document specifies
            // the object holding the flat fields
            // e.g. { "metrics.service": { "time.max" : 10 } } with service having subobjects set to false
            if (isWithinLeafObject.getAsBoolean()) {
                return;
            }
            XContentParser delegate = delegate();
            String field = delegate.currentName();
            String[] subpaths = splitAndValidatePath(field);
            // Corner case: if the input has a single trailing '.', eg 'field.', then we will get a single
            // subpath due to the way String.split() works. We can only return fast here if this is not
            // the case
            // TODO make this case throw an error instead? https://github.com/elastic/elasticsearch/issues/28948
            if (subpaths.length == 1 && field.endsWith(".") == false) {
                return;
            }
            XContentLocation location = delegate.getTokenLocation();
            Token token = delegate.nextToken();
            if (token == Token.END_OBJECT || token == Token.END_ARRAY) {
                throw new IllegalStateException("Expecting START_OBJECT or START_ARRAY or VALUE but got [" + token + "]");
            } else {
                XContentParser subParser = token == Token.START_OBJECT || token == Token.START_ARRAY
                    ? new XContentSubParser(delegate)
                    : new SingletonValueXContentParser(delegate);
                parsers.push(new DotExpandingXContentParser(subParser, subpaths, location, isWithinLeafObject));
            }
        }

        @Override
        protected XContentParser delegate() {
            return parsers.peek();
        }

        @Override
        public Map<String, Object> map() throws IOException {
            throw new UnsupportedOperationException();
        }

        @Override
        public Map<String, Object> mapOrdered() throws IOException {
            throw new UnsupportedOperationException();
        }

        @Override
        public Map<String, String> mapStrings() throws IOException {
            throw new UnsupportedOperationException();
        }

        @Override
        public <T> Map<String, T> map(Supplier<Map<String, T>> mapFactory, CheckedFunction<XContentParser, T, IOException> mapValueParser)
            throws IOException {
            throw new UnsupportedOperationException();
        }

        @Override
        public List<Object> list() throws IOException {
            throw new UnsupportedOperationException();
        }

        @Override
        public List<Object> listOrderedMap() throws IOException {
            throw new UnsupportedOperationException();
        }
    }

    private static String[] splitAndValidatePath(String fieldName) {
        if (fieldName.isEmpty()) {
            throw new IllegalArgumentException("field name cannot be an empty string");
        }
        if (fieldName.contains(".") == false) {
            return new String[] { fieldName };
        }
        String[] parts = fieldName.split("\\.");
        if (parts.length == 0) {
            throw new IllegalArgumentException("field name cannot contain only dots");
        }

        for (String part : parts) {
            // check if the field name contains only whitespace
            if (part.isEmpty()) {
                throw new IllegalArgumentException("field name cannot contain only whitespace: ['" + fieldName + "']");
            }
            if (part.isBlank()) {
                throw new IllegalArgumentException(
                    "field name starting or ending with a [.] makes object resolution ambiguous: [" + fieldName + "]"
                );
            }
        }
        return parts;
    }

    /**
     * Wraps an XContentParser such that it re-interprets dots in field names as an object structure
     * @param in    the parser to wrap
     * @return  the wrapped XContentParser
     */
    static XContentParser expandDots(XContentParser in, BooleanSupplier isWithinLeafObject) throws IOException {
        return new WrappingParser(in, isWithinLeafObject);
    }

    private enum State {
        EXPANDING_START_OBJECT,
        PARSING_ORIGINAL_CONTENT,
        ENDING_EXPANDED_OBJECT
    }

    private final BooleanSupplier isWithinLeafObject;

    private String[] subPaths;
    private XContentLocation currentLocation;
    private int expandedTokens = 0;
    private int innerLevel = -1;
    private State state = State.EXPANDING_START_OBJECT;

    private DotExpandingXContentParser(
        XContentParser subparser,
        String[] subPaths,
        XContentLocation startLocation,
        BooleanSupplier isWithinLeafObject
    ) {
        super(subparser);
        this.subPaths = subPaths;
        this.currentLocation = startLocation;
        this.isWithinLeafObject = isWithinLeafObject;
    }

    @Override
    public Token nextToken() throws IOException {
        if (state == State.EXPANDING_START_OBJECT) {
            expandedTokens++;
            assert expandedTokens < subPaths.length * 2;
            if (expandedTokens == subPaths.length * 2 - 1) {
                state = State.PARSING_ORIGINAL_CONTENT;
                Token token = delegate().currentToken();
                if (token == Token.START_OBJECT || token == Token.START_ARRAY) {
                    innerLevel++;
                }
                return token;
            }
            // The expansion consists of adding pairs of START_OBJECT and FIELD_NAME tokens
            if (expandedTokens % 2 == 0) {
                int currentIndex = expandedTokens / 2;
                // if there's more than one element left to expand and the parent can't hold subobjects, we replace the array
                // e.g. metrics.service.time.max -> ["metrics", "service", "time.max"]
                if (currentIndex < subPaths.length - 1 && isWithinLeafObject.getAsBoolean()) {
                    String[] newSubPaths = new String[currentIndex + 1];
                    StringBuilder collapsedPath = new StringBuilder();
                    for (int i = 0; i < subPaths.length; i++) {
                        if (i < currentIndex) {
                            newSubPaths[i] = subPaths[i];
                        } else {
                            collapsedPath.append(subPaths[i]);
                            if (i < subPaths.length - 1) {
                                collapsedPath.append(".");
                            }
                        }
                    }
                    newSubPaths[currentIndex] = collapsedPath.toString();
                    subPaths = newSubPaths;
                }
                return Token.FIELD_NAME;
            }
            return Token.START_OBJECT;
        }
        if (state == State.PARSING_ORIGINAL_CONTENT) {
            Token token = delegate().nextToken();
            if (token == Token.START_OBJECT || token == Token.START_ARRAY) {
                innerLevel++;
            }
            if (token == Token.END_OBJECT || token == Token.END_ARRAY) {
                innerLevel--;
            }
            if (token != null) {
                return token;
            }
            currentLocation = getTokenLocation();
            state = State.ENDING_EXPANDED_OBJECT;
        }
        assert expandedTokens % 2 == 1;
        expandedTokens -= 2;
        return expandedTokens < 0 ? null : Token.END_OBJECT;
    }

    @Override
    public XContentLocation getTokenLocation() {
        if (state == State.PARSING_ORIGINAL_CONTENT) {
            return super.getTokenLocation();
        }
        return currentLocation;
    }

    @Override
    public Token currentToken() {
        return switch (state) {
            case EXPANDING_START_OBJECT -> expandedTokens % 2 == 1 ? Token.START_OBJECT : Token.FIELD_NAME;
            case ENDING_EXPANDED_OBJECT -> Token.END_OBJECT;
            case PARSING_ORIGINAL_CONTENT -> delegate().currentToken();
        };
    }

    @Override
    public String currentName() throws IOException {
        if (state == State.PARSING_ORIGINAL_CONTENT) {
            assert expandedTokens == subPaths.length * 2 - 1;
            // whenever we are parsing some inner object/array we can easily delegate to the inner parser
            // e.g. field.with.dots: { obj:{ parsing here } }
            if (innerLevel > 0) {
                return delegate().currentName();
            }
            Token token = currentToken();
            // if we are parsing the outer object/array, only at the start object/array we need to return
            // e.g. dots instead of field.with.dots otherwise we can simply delegate to the inner parser
            // which will do the right thing
            if (innerLevel == 0 && token != Token.START_OBJECT && token != Token.START_ARRAY) {
                return delegate().currentName();
            }
            // note that innerLevel can be -1 if there are no inner object/array e.g. field.with.dots: value
            // as well as while there is and we are parsing their END_OBJECT or END_ARRAY
        }
        return subPaths[expandedTokens / 2];
    }

    @Override
    public void skipChildren() throws IOException {
        if (state == State.EXPANDING_START_OBJECT) {
            delegate().skipChildren();
            state = State.ENDING_EXPANDED_OBJECT;
        }
        if (state == State.PARSING_ORIGINAL_CONTENT) {
            delegate().skipChildren();
        }
    }

    @Override
    public String textOrNull() throws IOException {
        if (state == State.EXPANDING_START_OBJECT) {
            throw new IllegalStateException("Can't get text on a " + currentToken() + " at " + getTokenLocation());
        }
        return super.textOrNull();
    }

    @Override
    public Number numberValue() throws IOException {
        if (state == State.EXPANDING_START_OBJECT) {
            throw new IllegalStateException("Can't get numeric value on a " + currentToken() + " at " + getTokenLocation());
        }
        return super.numberValue();
    }

    @Override
    public boolean booleanValue() throws IOException {
        if (state == State.EXPANDING_START_OBJECT) {
            throw new IllegalStateException("Can't get boolean value on a " + currentToken() + " at " + getTokenLocation());
        }
        return super.booleanValue();
    }

    private static class SingletonValueXContentParser extends FilterXContentParserWrapper {

        protected SingletonValueXContentParser(XContentParser in) {
            super(in);
        }

        @Override
        public Token nextToken() throws IOException {
            return null;
        }
    }
}
