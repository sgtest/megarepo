/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.index.IndexableField;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.search.Query;
import org.elasticsearch.Version;
import org.elasticsearch.common.Explicit;
import org.elasticsearch.common.regex.Regex;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.index.fielddata.FieldDataContext;
import org.elasticsearch.index.fielddata.IndexFieldDataCache;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.indices.breaker.NoneCircuitBreakerService;
import org.elasticsearch.search.lookup.SearchLookup;
import org.elasticsearch.search.lookup.Source;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentParser;
import org.elasticsearch.xcontent.XContentParserConfiguration;
import org.elasticsearch.xcontent.XContentType;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;

/**
 * A parser for documents
 */
public final class DocumentParser {

    private final XContentParserConfiguration parserConfiguration;
    private final MappingParserContext mappingParserContext;

    DocumentParser(XContentParserConfiguration parserConfiguration, MappingParserContext mappingParserContext) {
        this.mappingParserContext = mappingParserContext;
        this.parserConfiguration = parserConfiguration;
    }

    /**
     * Parse a document
     *
     * @param source        the document to parse
     * @param mappingLookup the mappings information needed to parse the document
     * @return the parsed document
     * @throws MapperParsingException whenever there's a problem parsing the document
     */
    public ParsedDocument parseDocument(SourceToParse source, MappingLookup mappingLookup) throws MapperParsingException {
        final InternalDocumentParserContext context;
        final XContentType xContentType = source.getXContentType();
        try (XContentParser parser = XContentHelper.createParser(parserConfiguration, source.source(), xContentType)) {
            context = new InternalDocumentParserContext(mappingLookup, mappingParserContext, source, parser);
            validateStart(context.parser());
            MetadataFieldMapper[] metadataFieldsMappers = mappingLookup.getMapping().getSortedMetadataMappers();
            internalParseDocument(mappingLookup.getMapping().getRoot(), metadataFieldsMappers, context);
            validateEnd(context.parser());
        } catch (Exception e) {
            throw wrapInMapperParsingException(source, e);
        }
        String remainingPath = context.path().pathAsText("");
        if (remainingPath.isEmpty() == false) {
            throwOnLeftoverPathElements(remainingPath);
        }

        return new ParsedDocument(
            context.version(),
            context.seqID(),
            context.id(),
            source.routing(),
            context.reorderParentAndGetDocs(),
            context.sourceToParse().source(),
            context.sourceToParse().getXContentType(),
            createDynamicUpdate(context)
        ) {
            @Override
            public String documentDescription() {
                IdFieldMapper idMapper = (IdFieldMapper) mappingLookup.getMapping().getMetadataMapperByName(IdFieldMapper.NAME);
                return idMapper.documentDescription(this);
            }
        };
    }

    private static void throwOnLeftoverPathElements(String remainingPath) {
        throw new IllegalStateException("found leftover path elements: " + remainingPath);
    }

    private static void internalParseDocument(
        RootObjectMapper root,
        MetadataFieldMapper[] metadataFieldsMappers,
        DocumentParserContext context
    ) throws IOException {

        final boolean emptyDoc = isEmptyDoc(root, context.parser());

        for (MetadataFieldMapper metadataMapper : metadataFieldsMappers) {
            metadataMapper.preParse(context);
        }

        if (root.isEnabled() == false) {
            // entire type is disabled
            context.parser().skipChildren();
        } else if (emptyDoc == false) {
            parseObjectOrNested(context, root);
        }

        executeIndexTimeScripts(context);

        for (MetadataFieldMapper metadataMapper : metadataFieldsMappers) {
            metadataMapper.postParse(context);
        }
    }

    private static void executeIndexTimeScripts(DocumentParserContext context) {
        List<FieldMapper> indexTimeScriptMappers = context.mappingLookup().indexTimeScriptMappers();
        if (indexTimeScriptMappers.isEmpty()) {
            return;
        }
        SearchLookup searchLookup = new SearchLookup(
            context.mappingLookup().indexTimeLookup()::get,
            (ft, lookup, fto) -> ft.fielddataBuilder(
                new FieldDataContext(context.indexSettings().getIndex().getName(), lookup, context.mappingLookup()::sourcePaths, fto)
            ).build(new IndexFieldDataCache.None(), new NoneCircuitBreakerService()),
            (ctx, doc) -> Source.fromBytes(context.sourceToParse().source())
        );
        // field scripts can be called both by the loop at the end of this method and via
        // the document reader, so to ensure that we don't run them multiple times we
        // guard them with an 'executed' boolean
        Map<String, Consumer<LeafReaderContext>> fieldScripts = new HashMap<>();
        indexTimeScriptMappers.forEach(mapper -> fieldScripts.put(mapper.name(), new Consumer<>() {
            boolean executed = false;

            @Override
            public void accept(LeafReaderContext leafReaderContext) {
                if (executed == false) {
                    mapper.executeScript(searchLookup, leafReaderContext, 0, context);
                    executed = true;
                }
            }
        }));

        // call the index script on all field mappers configured with one
        DocumentLeafReader reader = new DocumentLeafReader(context.rootDoc(), fieldScripts);
        for (Consumer<LeafReaderContext> script : fieldScripts.values()) {
            script.accept(reader.getContext());
        }
    }

    private static void validateStart(XContentParser parser) throws IOException {
        // will result in START_OBJECT
        XContentParser.Token token = parser.nextToken();
        if (token != XContentParser.Token.START_OBJECT) {
            throwNoStartOnObject();
        }
    }

    private static void throwNoStartOnObject() {
        throw new MapperParsingException("Malformed content, must start with an object");
    }

    private static void validateEnd(XContentParser parser) throws IOException {
        XContentParser.Token token;// only check for end of tokens if we created the parser here
        // try to parse the next token, this should be null if the object is ended properly
        // but will throw a JSON exception if the extra tokens is not valid JSON (this will be handled by the catch)
        token = parser.nextToken();
        if (token != null) {
            throwNotAtEnd(token);
        }
    }

    private static void throwNotAtEnd(XContentParser.Token token) {
        throw new IllegalArgumentException("Malformed content, found extra data after parsing: " + token);
    }

    private static boolean isEmptyDoc(RootObjectMapper root, XContentParser parser) throws IOException {
        if (root.isEnabled()) {
            final XContentParser.Token token = parser.nextToken();
            switch (token) {
                case END_OBJECT:
                    // empty doc, we can handle it...
                    return true;
                case FIELD_NAME:
                    return false;
                default:
                    throwOnMalformedContent();
            }
        }
        return false;
    }

    private static void throwOnMalformedContent() {
        throw new MapperParsingException(
            "Malformed content, after first object, either the type field or the actual properties should exist"
        );
    }

    private static MapperParsingException wrapInMapperParsingException(SourceToParse source, Exception e) {
        // if its already a mapper parsing exception, no need to wrap it...
        if (e instanceof MapperParsingException) {
            return (MapperParsingException) e;
        }

        // Throw a more meaningful message if the document is empty.
        if (source.source() != null && source.source().length() == 0) {
            return new MapperParsingException("failed to parse, document is empty");
        }

        return new MapperParsingException("failed to parse", e);
    }

    static Mapping createDynamicUpdate(DocumentParserContext context) {
        if (context.getDynamicMappers().isEmpty() && context.getDynamicRuntimeFields().isEmpty()) {
            return null;
        }
        RootObjectMapper.Builder rootBuilder = context.updateRoot();
        for (Mapper mapper : context.getDynamicMappers()) {
            rootBuilder.addDynamic(mapper.name(), null, mapper, context);
        }
        for (RuntimeField runtimeField : context.getDynamicRuntimeFields()) {
            rootBuilder.addRuntimeField(runtimeField);
        }
        RootObjectMapper root = rootBuilder.build(MapperBuilderContext.root(context.mappingLookup().isSourceSynthetic()));
        return context.mappingLookup().getMapping().mappingUpdate(root);
    }

    static void parseObjectOrNested(DocumentParserContext context, ObjectMapper mapper) throws IOException {
        if (mapper.isEnabled() == false) {
            context.parser().skipChildren();
            return;
        }
        XContentParser parser = context.parser();
        XContentParser.Token token = parser.currentToken();
        if (token == XContentParser.Token.VALUE_NULL) {
            // the object is null ("obj1" : null), simply bail
            return;
        }

        String currentFieldName = parser.currentName();
        if (token.isValue()) {
            throwOnConcreteValue(mapper, currentFieldName);
        }

        if (mapper.isNested()) {
            context = context.createNestedContext((NestedObjectMapper) mapper);
        }

        // if we are at the end of the previous object, advance
        if (token == XContentParser.Token.END_OBJECT) {
            token = parser.nextToken();
        }
        if (token == XContentParser.Token.START_OBJECT) {
            // if we are just starting an OBJECT, advance, this is the object we are parsing, we need the name first
            parser.nextToken();
        }

        innerParseObject(context, mapper);
        // restore the enable path flag
        if (mapper.isNested()) {
            copyNestedFields(context, (NestedObjectMapper) mapper);
        }
    }

    private static void throwOnConcreteValue(ObjectMapper mapper, String currentFieldName) {
        throw new MapperParsingException(
            "object mapping for ["
                + mapper.name()
                + "] tried to parse field ["
                + currentFieldName
                + "] as object, but found a concrete value"
        );
    }

    private static void innerParseObject(DocumentParserContext context, ObjectMapper mapper) throws IOException {

        final XContentParser parser = context.parser();
        XContentParser.Token token = parser.currentToken();
        String currentFieldName = null;
        assert token == XContentParser.Token.FIELD_NAME || token == XContentParser.Token.END_OBJECT;

        while (token != XContentParser.Token.END_OBJECT) {
            if (token == null) {
                throwEOF(mapper, context);
            }
            switch (token) {
                case FIELD_NAME:
                    currentFieldName = parser.currentName();
                    if (currentFieldName.isEmpty()) {
                        throw new IllegalArgumentException("Field name cannot be an empty string");
                    }
                    if (currentFieldName.isBlank()) {
                        throwFieldNameBlank(context, currentFieldName);
                    }
                    break;
                case START_OBJECT:
                    parseObject(context, mapper, currentFieldName);
                    break;
                case START_ARRAY:
                    parseArray(context, mapper, currentFieldName);
                    break;
                case VALUE_NULL:
                    parseNullValue(context, mapper, currentFieldName);
                    break;
                default:
                    if (token.isValue()) {
                        parseValue(context, mapper, currentFieldName);
                    }
                    break;
            }
            token = parser.nextToken();
        }
    }

    private static void throwFieldNameBlank(DocumentParserContext context, String currentFieldName) {
        throw new MapperParsingException(
            "Field name cannot contain only whitespace: [" + context.path().pathAsText(currentFieldName) + "]"
        );
    }

    private static void throwEOF(ObjectMapper mapper, DocumentParserContext context) throws IOException {
        throw new MapperParsingException(
            "object mapping for ["
                + mapper.name()
                + "] tried to parse field ["
                + context.parser().currentName()
                + "] as object, but got EOF, has a concrete value been provided to it?"
        );
    }

    private static void copyNestedFields(DocumentParserContext context, NestedObjectMapper nested) {
        if (context.isWithinCopyTo()) {
            // Only process the nested document after we've finished parsing the actual
            // doc; we can't copy_to outside of the current nested context, so if we are
            // in a copy_to context then we're adding data within the doc and we haven't
            // finished parsing yet.
            return;
        }
        LuceneDocument nestedDoc = context.doc();
        LuceneDocument parentDoc = nestedDoc.getParent();
        Version indexVersion = context.indexSettings().getIndexVersionCreated();
        if (nested.isIncludeInParent()) {
            addFields(indexVersion, nestedDoc, parentDoc);
        }
        if (nested.isIncludeInRoot()) {
            LuceneDocument rootDoc = context.rootDoc();
            // don't add it twice, if its included in parent, and we are handling the master doc...
            if (nested.isIncludeInParent() == false || parentDoc != rootDoc) {
                addFields(indexVersion, nestedDoc, rootDoc);
            }
        }
    }

    private static void addFields(Version indexCreatedVersion, LuceneDocument nestedDoc, LuceneDocument rootDoc) {
        String nestedPathFieldName = NestedPathFieldMapper.name(indexCreatedVersion);
        for (IndexableField field : nestedDoc.getFields()) {
            if (field.name().equals(nestedPathFieldName) == false && field.name().equals(IdFieldMapper.NAME) == false) {
                rootDoc.add(field);
            }
        }
    }

    static void parseObjectOrField(DocumentParserContext context, Mapper mapper) throws IOException {
        if (mapper instanceof ObjectMapper objectMapper) {
            parseObjectOrNested(context, objectMapper);
        } else if (mapper instanceof FieldMapper fieldMapper) {
            fieldMapper.parse(context);
            if (context.isWithinCopyTo() == false) {
                List<String> copyToFields = fieldMapper.copyTo().copyToFields();
                if (copyToFields.isEmpty() == false) {
                    XContentParser.Token currentToken = context.parser().currentToken();
                    if (currentToken.isValue() == false && currentToken != XContentParser.Token.VALUE_NULL) {
                        // sanity check, we currently support copy-to only for value-type field, not objects
                        throwOnCopyToOnObject(mapper, copyToFields);
                    }
                    parseCopyFields(context, copyToFields);
                }
            }
        } else if (mapper instanceof FieldAliasMapper) {
            throwOnCopyToOnFieldAlias(context, mapper);
        } else {
            throwOnUnrecognizedMapperType(mapper);
        }
    }

    private static void throwOnUnrecognizedMapperType(Mapper mapper) {
        throw new IllegalStateException(
            "The provided mapper [" + mapper.name() + "] has an unrecognized type [" + mapper.getClass().getSimpleName() + "]."
        );
    }

    private static void throwOnCopyToOnFieldAlias(DocumentParserContext context, Mapper mapper) {
        throw new MapperParsingException(
            "Cannot " + (context.isWithinCopyTo() ? "copy" : "write") + " to a field alias [" + mapper.name() + "]."
        );
    }

    private static void throwOnCopyToOnObject(Mapper mapper, List<String> copyToFields) {
        throw new MapperParsingException(
            "Cannot copy field ["
                + mapper.name()
                + "] to fields "
                + copyToFields
                + ". Copy-to currently only works for value-type fields, not objects."
        );
    }

    private static void parseObject(final DocumentParserContext context, ObjectMapper parentObjectMapper, String currentFieldName)
        throws IOException {
        assert currentFieldName != null;
        Mapper objectMapper = getMapper(context, parentObjectMapper, currentFieldName);
        if (objectMapper != null) {
            context.path().add(currentFieldName);
            if (objectMapper instanceof ObjectMapper objMapper) {
                if (objMapper.subobjects() == false) {
                    context.path().setWithinLeafObject(true);
                }
            }
            parseObjectOrField(context, objectMapper);
            context.path().setWithinLeafObject(false);
            context.path().remove();
        } else {
            parseObjectDynamic(context, parentObjectMapper, currentFieldName);
        }
    }

    private static void parseObjectDynamic(DocumentParserContext context, ObjectMapper parentObjectMapper, String currentFieldName)
        throws IOException {
        ObjectMapper.Dynamic dynamic = dynamicOrDefault(parentObjectMapper, context);
        if (dynamic == ObjectMapper.Dynamic.STRICT) {
            throw new StrictDynamicMappingException(parentObjectMapper.fullPath(), currentFieldName);
        } else if (dynamic == ObjectMapper.Dynamic.FALSE) {
            failIfMatchesRoutingPath(context, parentObjectMapper, currentFieldName);
            // not dynamic, read everything up to end object
            context.parser().skipChildren();
        } else {
            Mapper dynamicObjectMapper;
            if (dynamic == ObjectMapper.Dynamic.RUNTIME) {
                // with dynamic:runtime all leaf fields will be runtime fields unless explicitly mapped,
                // hence we don't dynamically create empty objects under properties, but rather carry around an artificial object mapper
                dynamicObjectMapper = new NoOpObjectMapper(currentFieldName, context.path().pathAsText(currentFieldName));
            } else {
                dynamicObjectMapper = DynamicFieldsBuilder.createDynamicObjectMapper(context, currentFieldName);
                context.addDynamicMapper(dynamicObjectMapper);
            }
            if (parentObjectMapper.subobjects() == false) {
                if (dynamicObjectMapper instanceof NestedObjectMapper) {
                    throw new MapperParsingException(
                        "Tried to add nested object ["
                            + dynamicObjectMapper.simpleName()
                            + "] to object ["
                            + parentObjectMapper.name()
                            + "] which does not support subobjects"
                    );
                }
                if (dynamicObjectMapper instanceof ObjectMapper) {
                    throw new MapperParsingException(
                        "Tried to add subobject ["
                            + dynamicObjectMapper.simpleName()
                            + "] to object ["
                            + parentObjectMapper.name()
                            + "] which does not support subobjects"
                    );
                }
            }
            if (dynamicObjectMapper instanceof NestedObjectMapper && context.isWithinCopyTo()) {
                throwOnCreateDynamicNestedViaCopyTo(dynamicObjectMapper);
            }
            context.path().add(currentFieldName);
            if (dynamicObjectMapper instanceof ObjectMapper objectMapper) {
                if (objectMapper.subobjects() == false) {
                    context.path().setWithinLeafObject(true);
                }
            }
            parseObjectOrField(context, dynamicObjectMapper);
            context.path().setWithinLeafObject(false);
            context.path().remove();
        }
    }

    private static void throwOnCreateDynamicNestedViaCopyTo(Mapper dynamicObjectMapper) {
        throw new MapperParsingException(
            "It is forbidden to create dynamic nested objects ([" + dynamicObjectMapper.name() + "]) through `copy_to`"
        );
    }

    private static void parseArray(DocumentParserContext context, ObjectMapper parentMapper, String lastFieldName) throws IOException {
        Mapper mapper = getLeafMapper(context, parentMapper, lastFieldName);
        if (mapper != null) {
            // There is a concrete mapper for this field already. Need to check if the mapper
            // expects an array, if so we pass the context straight to the mapper and if not
            // we serialize the array components
            if (parsesArrayValue(mapper)) {
                parseObjectOrField(context, mapper);
            } else {
                parseNonDynamicArray(context, parentMapper, lastFieldName, lastFieldName);
            }
        } else {
            ObjectMapper.Dynamic dynamic = dynamicOrDefault(parentMapper, context);
            if (dynamic == ObjectMapper.Dynamic.STRICT) {
                throw new StrictDynamicMappingException(parentMapper.fullPath(), lastFieldName);
            } else if (dynamic == ObjectMapper.Dynamic.FALSE) {
                context.parser().skipChildren();
            } else {
                Mapper objectMapperFromTemplate = DynamicFieldsBuilder.createObjectMapperFromTemplate(context, lastFieldName);
                if (objectMapperFromTemplate == null) {
                    parseNonDynamicArray(context, parentMapper, lastFieldName, lastFieldName);
                } else {
                    if (parsesArrayValue(objectMapperFromTemplate)) {
                        context.addDynamicMapper(objectMapperFromTemplate);
                        context.path().add(lastFieldName);
                        parseObjectOrField(context, objectMapperFromTemplate);
                        context.path().remove();
                    } else {
                        parseNonDynamicArray(context, parentMapper, lastFieldName, lastFieldName);
                    }
                }
            }
        }
    }

    private static boolean parsesArrayValue(Mapper mapper) {
        return mapper instanceof FieldMapper && ((FieldMapper) mapper).parsesArrayValue();
    }

    private static void parseNonDynamicArray(
        DocumentParserContext context,
        ObjectMapper mapper,
        final String lastFieldName,
        String arrayFieldName
    ) throws IOException {
        XContentParser parser = context.parser();
        XContentParser.Token token;
        while ((token = parser.nextToken()) != XContentParser.Token.END_ARRAY) {
            if (token == XContentParser.Token.START_OBJECT) {
                parseObject(context, mapper, lastFieldName);
            } else if (token == XContentParser.Token.START_ARRAY) {
                parseArray(context, mapper, lastFieldName);
            } else if (token == XContentParser.Token.VALUE_NULL) {
                parseNullValue(context, mapper, lastFieldName);
            } else if (token == null) {
                throwEOFOnParseArray(mapper, arrayFieldName);
            } else {
                assert token.isValue();
                parseValue(context, mapper, lastFieldName);
            }
        }
    }

    private static void throwEOFOnParseArray(ObjectMapper mapper, String arrayFieldName) {
        throw new MapperParsingException(
            "object mapping for ["
                + mapper.name()
                + "] with array for ["
                + arrayFieldName
                + "] tried to parse as array, but got EOF, is there a mismatch in types for the same field?"
        );
    }

    private static void parseValue(final DocumentParserContext context, ObjectMapper parentMapper, String currentFieldName)
        throws IOException {
        if (currentFieldName == null) {
            throwOnNoFieldName(context, parentMapper);
        }
        Mapper mapper = getLeafMapper(context, parentMapper, currentFieldName);
        if (mapper != null) {
            parseObjectOrField(context, mapper);
        } else {
            parseDynamicValue(context, parentMapper, currentFieldName);
        }
    }

    private static void throwOnNoFieldName(DocumentParserContext context, ObjectMapper parentMapper) throws IOException {
        throw new MapperParsingException(
            "object mapping ["
                + parentMapper.name()
                + "] trying to serialize a value with"
                + " no field associated with it, current value ["
                + context.parser().textOrNull()
                + "]"
        );
    }

    private static void parseNullValue(DocumentParserContext context, ObjectMapper parentMapper, String lastFieldName) throws IOException {
        // we can only handle null values if we have mappings for them
        Mapper mapper = getLeafMapper(context, parentMapper, lastFieldName);
        if (mapper != null) {
            // TODO: passing null to an object seems bogus?
            parseObjectOrField(context, mapper);
        } else if (parentMapper.dynamic() == ObjectMapper.Dynamic.STRICT) {
            throw new StrictDynamicMappingException(parentMapper.fullPath(), lastFieldName);
        }
    }

    private static void parseDynamicValue(final DocumentParserContext context, ObjectMapper parentMapper, String currentFieldName)
        throws IOException {
        ObjectMapper.Dynamic dynamic = dynamicOrDefault(parentMapper, context);
        if (dynamic == ObjectMapper.Dynamic.STRICT) {
            throw new StrictDynamicMappingException(parentMapper.fullPath(), currentFieldName);
        }
        if (dynamic == ObjectMapper.Dynamic.FALSE) {
            failIfMatchesRoutingPath(context, parentMapper, currentFieldName);
            return;
        }
        dynamic.getDynamicFieldsBuilder().createDynamicFieldFromValue(context, currentFieldName);
    }

    private static void failIfMatchesRoutingPath(DocumentParserContext context, ObjectMapper parentMapper, String currentFieldName) {
        if (context.indexSettings().getIndexMetadata().getRoutingPaths().isEmpty()) {
            return;
        }
        String path = parentMapper.fullPath().isEmpty() ? currentFieldName : parentMapper.fullPath() + "." + currentFieldName;
        if (Regex.simpleMatch(context.indexSettings().getIndexMetadata().getRoutingPaths(), path)) {
            throw new MapperParsingException(
                "All fields matching [routing_path] must be mapped but [" + path + "] was declared as [dynamic: false]"
            );
        }
    }

    /**
     * Creates instances of the fields that the current field should be copied to
     */
    private static void parseCopyFields(DocumentParserContext context, List<String> copyToFields) throws IOException {
        for (String field : copyToFields) {
            // In case of a hierarchy of nested documents, we need to figure out
            // which document the field should go to
            LuceneDocument targetDoc = null;
            for (LuceneDocument doc = context.doc(); doc != null; doc = doc.getParent()) {
                if (field.startsWith(doc.getPrefix())) {
                    targetDoc = doc;
                    break;
                }
            }
            assert targetDoc != null;
            final DocumentParserContext copyToContext = context.createCopyToContext(field, targetDoc);
            innerParseObject(copyToContext, context.root());
        }
    }

    // find what the dynamic setting is given the current parse context and parent
    private static ObjectMapper.Dynamic dynamicOrDefault(ObjectMapper parentMapper, DocumentParserContext context) {
        ObjectMapper.Dynamic dynamic = parentMapper.dynamic();
        while (dynamic == null) {
            int lastDotNdx = parentMapper.name().lastIndexOf('.');
            if (lastDotNdx == -1) {
                // no dot means we the parent is the root, so just delegate to the default outside the loop
                break;
            }
            String parentName = parentMapper.name().substring(0, lastDotNdx);
            parentMapper = context.mappingLookup().objectMappers().get(parentName);
            if (parentMapper == null) {
                // If parentMapper is null, it means the parent of the current mapper is being dynamically created right now
                parentMapper = context.getDynamicObjectMapper(parentName);
                if (parentMapper == null) {
                    // it can still happen that the path is ambiguous and we are not able to locate the parent
                    break;
                }
            }
            dynamic = parentMapper.dynamic();
        }
        if (dynamic == null) {
            return context.root().dynamic() == null ? ObjectMapper.Dynamic.TRUE : context.root().dynamic();
        }
        return dynamic;
    }

    // looks up a child mapper
    // returns null if no such child mapper exists - note that unlike getLeafMapper,
    // we do not check for runtime fields with same name as they only apply to leaf
    // fields
    private static Mapper getMapper(final DocumentParserContext context, ObjectMapper objectMapper, String fieldName) {
        if (context.path().atRoot()) {
            // Check if mapper is a metadata mapper first
            Mapper mapper = context.getMetadataMapper(fieldName);
            if (mapper != null) {
                return mapper;
            }
        }
        return objectMapper.getMapper(fieldName);
    }

    // looks up a child mapper
    // if no mapper is found, checks to see if a runtime field with the specified
    // field name exists and if so returns a no-op mapper to prevent indexing
    private static Mapper getLeafMapper(final DocumentParserContext context, ObjectMapper objectMapper, String fieldName) {
        Mapper mapper = getMapper(context, objectMapper, fieldName);
        if (mapper != null) {
            return mapper;
        }
        // concrete fields take precedence over runtime fields when parsing documents
        // if a leaf field is not mapped, and is defined as a runtime field, then we
        // don't create a dynamic mapping for it and don't index it.
        String fieldPath = context.path().pathAsText(fieldName);
        MappedFieldType fieldType = context.mappingLookup().getFieldType(fieldPath);
        if (fieldType != null) {
            // we haven't found a mapper with this name above, which means if a field type is found it is for sure a runtime field.
            assert fieldType.hasDocValues() == false && fieldType.isAggregatable() && fieldType.isSearchable();
            return NO_OP_FIELDMAPPER;
        }
        return null;
    }

    private static final FieldMapper NO_OP_FIELDMAPPER = new FieldMapper(
        "no-op",
        new MappedFieldType("no-op", false, false, false, TextSearchInfo.NONE, Collections.emptyMap()) {
            @Override
            public ValueFetcher valueFetcher(SearchExecutionContext context, String format) {
                throw new UnsupportedOperationException();
            }

            @Override
            public String typeName() {
                throw new UnsupportedOperationException();
            }

            @Override
            public Query termQuery(Object value, SearchExecutionContext context) {
                throw new UnsupportedOperationException();
            }
        },
        FieldMapper.MultiFields.empty(),
        FieldMapper.CopyTo.empty()
    ) {

        @Override
        protected void parseCreateField(DocumentParserContext context) throws IOException {
            // field defined as runtime field, don't index anything
        }

        @Override
        public String name() {
            throw new UnsupportedOperationException();
        }

        @Override
        public String typeName() {
            throw new UnsupportedOperationException();
        }

        @Override
        public MappedFieldType fieldType() {
            throw new UnsupportedOperationException();
        }

        @Override
        public MultiFields multiFields() {
            throw new UnsupportedOperationException();
        }

        @Override
        public Iterator<Mapper> iterator() {
            throw new UnsupportedOperationException();
        }

        @Override
        protected void doValidate(MappingLookup mappers) {
            throw new UnsupportedOperationException();
        }

        @Override
        protected void checkIncomingMergeType(FieldMapper mergeWith) {
            throw new UnsupportedOperationException();
        }

        @Override
        public Builder getMergeBuilder() {
            throw new UnsupportedOperationException();
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            throw new UnsupportedOperationException();
        }

        @Override
        protected String contentType() {
            throw new UnsupportedOperationException();
        }
    };

    private static class NoOpObjectMapper extends ObjectMapper {
        NoOpObjectMapper(String name, String fullPath) {
            super(name, fullPath, Explicit.IMPLICIT_TRUE, Explicit.IMPLICIT_TRUE, Dynamic.RUNTIME, Collections.emptyMap());
        }
    }

    /**
     * Internal version of {@link DocumentParserContext} that is aware of implementation details like nested documents
     * and how they are stored in the lucene index.
     */
    private static class InternalDocumentParserContext extends DocumentParserContext {
        private final ContentPath path = new ContentPath(0);
        private final XContentParser parser;
        private final LuceneDocument document;
        private final List<LuceneDocument> documents = new ArrayList<>();
        private final long maxAllowedNumNestedDocs;
        private long numNestedDocs;
        private boolean docsReversed = false;

        InternalDocumentParserContext(
            MappingLookup mappingLookup,
            MappingParserContext mappingParserContext,
            SourceToParse source,
            XContentParser parser
        ) throws IOException {
            super(mappingLookup, mappingParserContext, source);
            if (mappingLookup.getMapping().getRoot().subobjects()) {
                this.parser = DotExpandingXContentParser.expandDots(parser, this.path::isWithinLeafObject);
            } else {
                this.parser = parser;
            }
            this.document = new LuceneDocument();
            this.documents.add(document);
            this.maxAllowedNumNestedDocs = indexSettings().getMappingNestedDocsLimit();
            this.numNestedDocs = 0L;
        }

        @Override
        public ContentPath path() {
            return this.path;
        }

        @Override
        public XContentParser parser() {
            return this.parser;
        }

        @Override
        public LuceneDocument rootDoc() {
            return documents.get(0);
        }

        @Override
        public LuceneDocument doc() {
            return this.document;
        }

        @Override
        protected void addDoc(LuceneDocument doc) {
            numNestedDocs++;
            if (numNestedDocs > maxAllowedNumNestedDocs) {
                throw new MapperParsingException(
                    "The number of nested documents has exceeded the allowed limit of ["
                        + maxAllowedNumNestedDocs
                        + "]."
                        + " This limit can be set by changing the ["
                        + MapperService.INDEX_MAPPING_NESTED_DOCS_LIMIT_SETTING.getKey()
                        + "] index level setting."
                );
            }
            this.documents.add(doc);
        }

        @Override
        public Iterable<LuceneDocument> nonRootDocuments() {
            if (docsReversed) {
                throw new IllegalStateException("documents are already reversed");
            }
            return documents.subList(1, documents.size());
        }

        /**
         * Returns a copy of the provided {@link List} where parent documents appear
         * after their children.
         */
        private List<LuceneDocument> reorderParentAndGetDocs() {
            if (documents.size() > 1 && docsReversed == false) {
                docsReversed = true;
                // We preserve the order of the children while ensuring that parents appear after them.
                List<LuceneDocument> newDocs = new ArrayList<>(documents.size());
                LinkedList<LuceneDocument> parents = new LinkedList<>();
                for (LuceneDocument doc : documents) {
                    while (parents.peek() != doc.getParent()) {
                        newDocs.add(parents.poll());
                    }
                    parents.add(0, doc);
                }
                newDocs.addAll(parents);
                documents.clear();
                documents.addAll(newDocs);
            }
            return documents;
        }
    }
}
