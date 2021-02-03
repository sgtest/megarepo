/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.ingest;

import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.VersionType;
import org.elasticsearch.ingest.ConfigurationUtils;
import org.elasticsearch.ingest.IngestDocument;
import org.elasticsearch.ingest.IngestDocument.Metadata;
import org.elasticsearch.ingest.IngestService;
import org.elasticsearch.ingest.Pipeline;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;

public class SimulatePipelineRequest extends ActionRequest implements ToXContentObject {
    private String id;
    private boolean verbose;
    private BytesReference source;
    private XContentType xContentType;

    /**
     * Creates a new request with the given source and its content type
     */
    public SimulatePipelineRequest(BytesReference source, XContentType xContentType) {
        this.source = Objects.requireNonNull(source);
        this.xContentType = Objects.requireNonNull(xContentType);
    }

    SimulatePipelineRequest() {
    }

    SimulatePipelineRequest(StreamInput in) throws IOException {
        super(in);
        id = in.readOptionalString();
        verbose = in.readBoolean();
        source = in.readBytesReference();
        xContentType = in.readEnum(XContentType.class);
    }

    @Override
    public ActionRequestValidationException validate() {
        return null;
    }

    public String getId() {
        return id;
    }

    public void setId(String id) {
        this.id = id;
    }

    public boolean isVerbose() {
        return verbose;
    }

    public void setVerbose(boolean verbose) {
        this.verbose = verbose;
    }

    public BytesReference getSource() {
        return source;
    }

    public XContentType getXContentType() {
        return xContentType;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        super.writeTo(out);
        out.writeOptionalString(id);
        out.writeBoolean(verbose);
        out.writeBytesReference(source);
        XContentHelper.writeTo(out, xContentType);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.rawValue(source.streamInput(), xContentType);
        return builder;
    }

    public static final class Fields {
        static final String PIPELINE = "pipeline";
        static final String DOCS = "docs";
        static final String SOURCE = "_source";
    }

    static class Parsed {
        private final List<IngestDocument> documents;
        private final Pipeline pipeline;
        private final boolean verbose;

        Parsed(Pipeline pipeline, List<IngestDocument> documents, boolean verbose) {
            this.pipeline = pipeline;
            this.documents = Collections.unmodifiableList(documents);
            this.verbose = verbose;
        }

        public Pipeline getPipeline() {
            return pipeline;
        }

        public List<IngestDocument> getDocuments() {
            return documents;
        }

        public boolean isVerbose() {
            return verbose;
        }
    }

    static final String SIMULATED_PIPELINE_ID = "_simulate_pipeline";

    static Parsed parseWithPipelineId(String pipelineId, Map<String, Object> config, boolean verbose, IngestService ingestService) {
        if (pipelineId == null) {
            throw new IllegalArgumentException("param [pipeline] is null");
        }
        Pipeline pipeline = ingestService.getPipeline(pipelineId);
        if (pipeline == null) {
            throw new IllegalArgumentException("pipeline [" + pipelineId + "] does not exist");
        }
        List<IngestDocument> ingestDocumentList = parseDocs(config);
        return new Parsed(pipeline, ingestDocumentList, verbose);
    }

    static Parsed parse(Map<String, Object> config, boolean verbose, IngestService ingestService) throws Exception {
        Map<String, Object> pipelineConfig = ConfigurationUtils.readMap(null, null, config, Fields.PIPELINE);
        Pipeline pipeline = Pipeline.create(
            SIMULATED_PIPELINE_ID, pipelineConfig, ingestService.getProcessorFactories(), ingestService.getScriptService()
        );
        List<IngestDocument> ingestDocumentList = parseDocs(config);
        return new Parsed(pipeline, ingestDocumentList, verbose);
    }

    private static List<IngestDocument> parseDocs(Map<String, Object> config) {
        List<Map<String, Object>> docs =
            ConfigurationUtils.readList(null, null, config, Fields.DOCS);
        if (docs.isEmpty()) {
            throw new IllegalArgumentException("must specify at least one document in [docs]");
        }
        List<IngestDocument> ingestDocumentList = new ArrayList<>();
        for (Object object : docs) {
            if ((object instanceof Map) ==  false) {
                throw new IllegalArgumentException("malformed [docs] section, should include an inner object");
            }
            Map<String, Object> dataMap = (Map<String, Object>) object;
            Map<String, Object> document = ConfigurationUtils.readMap(null, null,
                dataMap, Fields.SOURCE);
            String index = ConfigurationUtils.readStringOrIntProperty(null, null,
                dataMap, Metadata.INDEX.getFieldName(), "_index");
            String id = ConfigurationUtils.readStringOrIntProperty(null, null,
                dataMap, Metadata.ID.getFieldName(), "_id");
            String routing = ConfigurationUtils.readOptionalStringOrIntProperty(null, null,
                dataMap, Metadata.ROUTING.getFieldName());
            Long version = null;
            if (dataMap.containsKey(Metadata.VERSION.getFieldName())) {
                String versionValue = ConfigurationUtils.readOptionalStringOrIntProperty(null, null,
                    dataMap, Metadata.VERSION.getFieldName());
                if (versionValue != null) {
                    version = Long.valueOf(versionValue);
                } else {
                    throw new IllegalArgumentException("[_version] cannot be null");
                }
            }
            VersionType versionType = null;
            if (dataMap.containsKey(Metadata.VERSION_TYPE.getFieldName())) {
                versionType = VersionType.fromString(ConfigurationUtils.readStringProperty(null, null, dataMap,
                    Metadata.VERSION_TYPE.getFieldName()));
            }
            IngestDocument ingestDocument =
                new IngestDocument(index, id, routing, version, versionType, document);
            if (dataMap.containsKey(Metadata.IF_SEQ_NO.getFieldName())) {
                String ifSeqNoValue = ConfigurationUtils.readOptionalStringOrIntProperty(null, null,
                    dataMap, Metadata.IF_SEQ_NO.getFieldName());
                if (ifSeqNoValue != null) {
                    Long ifSeqNo = Long.valueOf(ifSeqNoValue);
                    ingestDocument.setFieldValue(Metadata.IF_SEQ_NO.getFieldName(), ifSeqNo);
                } else {
                    throw new IllegalArgumentException("[_if_seq_no] cannot be null");
                }
            }
            if (dataMap.containsKey(Metadata.IF_PRIMARY_TERM.getFieldName())) {
                String ifPrimaryTermValue = ConfigurationUtils.readOptionalStringOrIntProperty(null, null,
                    dataMap, Metadata.IF_PRIMARY_TERM.getFieldName());
                if (ifPrimaryTermValue != null) {
                    Long ifPrimaryTerm = Long.valueOf(ifPrimaryTermValue);
                    ingestDocument.setFieldValue(Metadata.IF_PRIMARY_TERM.getFieldName(), ifPrimaryTerm);
                } else {
                    throw new IllegalArgumentException("[_if_primary_term] cannot be null");
                }
            }
            ingestDocumentList.add(ingestDocument);
        }
        return ingestDocumentList;
    }
}
