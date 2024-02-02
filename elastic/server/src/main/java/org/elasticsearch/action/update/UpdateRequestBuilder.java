/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.update;

import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.ActiveShardCount;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.action.support.WriteRequestBuilder;
import org.elasticsearch.action.support.replication.ReplicationRequest;
import org.elasticsearch.action.support.single.instance.InstanceShardOperationRequestBuilder;
import org.elasticsearch.client.internal.ElasticsearchClient;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.index.VersionType;
import org.elasticsearch.script.Script;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentType;

import java.util.Map;

public class UpdateRequestBuilder extends InstanceShardOperationRequestBuilder<UpdateRequest, UpdateResponse, UpdateRequestBuilder>
    implements
        WriteRequestBuilder<UpdateRequestBuilder> {

    private String id;
    private String routing;
    private Script script;

    private String fetchSourceInclude;
    private String fetchSourceExclude;
    private String[] fetchSourceIncludeArray;
    private String[] fetchSourceExcludeArray;
    private Boolean fetchSource;

    private Integer retryOnConflict;
    private Long version;
    private VersionType versionType;
    private Long ifSeqNo;
    private Long ifPrimaryTerm;
    private ActiveShardCount waitForActiveShards;

    private IndexRequest doc;
    private XContentBuilder docSourceXContentBuilder;
    private Map<String, Object> docSourceMap;
    private XContentType docSourceXContentType;
    private String docSourceString;
    private byte[] docSourceBytes;
    private Integer docSourceOffset;
    private Integer docSourceLength;
    private Object[] docSourceArray;

    private IndexRequest upsert;
    private XContentBuilder upsertSourceXContentBuilder;
    private Map<String, Object> upsertSourceMap;
    private XContentType upsertSourceXContentType;
    private String upsertSourceString;
    private byte[] upsertSourceBytes;
    private Integer upsertSourceOffset;
    private Integer upsertSourceLength;
    private Object[] upsertSourceArray;

    private Boolean docAsUpsert;
    private Boolean detectNoop;
    private Boolean scriptedUpsert;
    private Boolean requireAlias;
    private WriteRequest.RefreshPolicy refreshPolicy;
    private String refreshPolicyString;

    public UpdateRequestBuilder(ElasticsearchClient client) {
        this(client, null, null);
    }

    public UpdateRequestBuilder(ElasticsearchClient client, String index, String id) {
        super(client, TransportUpdateAction.TYPE);
        setIndex(index);
        setId(id);
    }

    /**
     * Sets the id of the indexed document.
     */
    public UpdateRequestBuilder setId(String id) {
        this.id = id;
        return this;
    }

    /**
     * Controls the shard routing of the request. Using this value to hash the shard
     * and not the id.
     */
    public UpdateRequestBuilder setRouting(String routing) {
        this.routing = routing;
        return this;
    }

    /**
     * The script to execute. Note, make sure not to send different script each times and instead
     * use script params if possible with the same (automatically compiled) script.
     * <p>
     * The script works with the variable <code>ctx</code>, which is bound to the entry,
     * e.g. <code>ctx._source.mycounter += 1</code>.
     *
     */
    public UpdateRequestBuilder setScript(Script script) {
        this.script = script;
        return this;
    }

    /**
     * Indicate that _source should be returned with every hit, with an
     * "include" and/or "exclude" set which can include simple wildcard
     * elements.
     *
     * @param include
     *            An optional include (optionally wildcarded) pattern to filter
     *            the returned _source
     * @param exclude
     *            An optional exclude (optionally wildcarded) pattern to filter
     *            the returned _source
     */
    public UpdateRequestBuilder setFetchSource(@Nullable String include, @Nullable String exclude) {
        this.fetchSourceInclude = include;
        this.fetchSourceExclude = exclude;
        return this;
    }

    /**
     * Indicate that _source should be returned, with an
     * "include" and/or "exclude" set which can include simple wildcard
     * elements.
     *
     * @param includes
     *            An optional list of include (optionally wildcarded) pattern to
     *            filter the returned _source
     * @param excludes
     *            An optional list of exclude (optionally wildcarded) pattern to
     *            filter the returned _source
     */
    public UpdateRequestBuilder setFetchSource(@Nullable String[] includes, @Nullable String[] excludes) {
        this.fetchSourceIncludeArray = includes;
        this.fetchSourceExcludeArray = excludes;
        return this;
    }

    /**
     * Indicates whether the response should contain the updated _source.
     */
    public UpdateRequestBuilder setFetchSource(boolean fetchSource) {
        this.fetchSource = fetchSource;
        return this;
    }

    /**
     * Sets the number of retries of a version conflict occurs because the document was updated between
     * getting it and updating it. Defaults to 0.
     */
    public UpdateRequestBuilder setRetryOnConflict(int retryOnConflict) {
        this.retryOnConflict = retryOnConflict;
        return this;
    }

    /**
     * Sets the version, which will cause the index operation to only be performed if a matching
     * version exists and no changes happened on the doc since then.
     */
    public UpdateRequestBuilder setVersion(long version) {
        this.version = version;
        return this;
    }

    /**
     * Sets the versioning type. Defaults to {@link org.elasticsearch.index.VersionType#INTERNAL}.
     */
    public UpdateRequestBuilder setVersionType(VersionType versionType) {
        this.versionType = versionType;
        return this;
    }

    /**
     * only perform this update request if the document was last modification was assigned the given
     * sequence number. Must be used in combination with {@link #setIfPrimaryTerm(long)}
     *
     * If the document last modification was assigned a different sequence number a
     * {@link org.elasticsearch.index.engine.VersionConflictEngineException} will be thrown.
     */
    public UpdateRequestBuilder setIfSeqNo(long seqNo) {
        this.ifSeqNo = seqNo;
        return this;
    }

    /**
     * only perform this update request if the document was last modification was assigned the given
     * primary term. Must be used in combination with {@link #setIfSeqNo(long)}
     *
     * If the document last modification was assigned a different term a
     * {@link org.elasticsearch.index.engine.VersionConflictEngineException} will be thrown.
     */
    public UpdateRequestBuilder setIfPrimaryTerm(long term) {
        this.ifPrimaryTerm = term;
        return this;
    }

    /**
     * Sets the number of shard copies that must be active before proceeding with the write.
     * See {@link ReplicationRequest#waitForActiveShards(ActiveShardCount)} for details.
     */
    public UpdateRequestBuilder setWaitForActiveShards(ActiveShardCount waitForActiveShards) {
        this.waitForActiveShards = waitForActiveShards;
        return this;
    }

    /**
     * A shortcut for {@link #setWaitForActiveShards(ActiveShardCount)} where the numerical
     * shard count is passed in, instead of having to first call {@link ActiveShardCount#from(int)}
     * to get the ActiveShardCount.
     */
    public UpdateRequestBuilder setWaitForActiveShards(final int waitForActiveShards) {
        return setWaitForActiveShards(ActiveShardCount.from(waitForActiveShards));
    }

    /**
     * Sets the doc to use for updates when a script is not specified.
     */
    public UpdateRequestBuilder setDoc(IndexRequest indexRequest) {
        this.doc = indexRequest;
        return this;
    }

    /**
     * Sets the doc to use for updates when a script is not specified.
     */
    public UpdateRequestBuilder setDoc(XContentBuilder source) {
        this.docSourceXContentBuilder = source;
        return this;
    }

    /**
     * Sets the doc to use for updates when a script is not specified.
     */
    public UpdateRequestBuilder setDoc(Map<String, Object> source) {
        this.docSourceMap = source;
        return this;
    }

    /**
     * Sets the doc to use for updates when a script is not specified.
     */
    public UpdateRequestBuilder setDoc(Map<String, Object> source, XContentType contentType) {
        this.docSourceMap = source;
        this.docSourceXContentType = contentType;
        return this;
    }

    /**
     * Sets the doc to use for updates when a script is not specified.
     */
    public UpdateRequestBuilder setDoc(String source, XContentType xContentType) {
        this.docSourceString = source;
        this.docSourceXContentType = xContentType;
        return this;
    }

    /**
     * Sets the doc to use for updates when a script is not specified.
     */
    public UpdateRequestBuilder setDoc(byte[] source, XContentType xContentType) {
        this.docSourceBytes = source;
        this.docSourceXContentType = xContentType;
        return this;
    }

    /**
     * Sets the doc to use for updates when a script is not specified.
     */
    public UpdateRequestBuilder setDoc(byte[] source, int offset, int length, XContentType xContentType) {
        this.docSourceBytes = source;
        this.docSourceOffset = offset;
        this.docSourceLength = length;
        this.docSourceXContentType = xContentType;
        return this;
    }

    /**
     * Sets the doc to use for updates when a script is not specified, the doc provided
     * is a field and value pairs.
     */
    public UpdateRequestBuilder setDoc(Object... source) {
        this.docSourceArray = source;
        return this;
    }

    /**
     * Sets the doc to use for updates when a script is not specified, the doc provided
     * is a field and value pairs.
     */
    public UpdateRequestBuilder setDoc(XContentType xContentType, Object... source) {
        this.docSourceArray = source;
        this.docSourceXContentType = xContentType;
        return this;
    }

    /**
     * Sets the index request to be used if the document does not exists. Otherwise, a
     * {@link org.elasticsearch.index.engine.DocumentMissingException} is thrown.
     */
    public UpdateRequestBuilder setUpsert(IndexRequest indexRequest) {
        this.upsert = indexRequest;
        return this;
    }

    /**
     * Sets the doc source of the update request to be used when the document does not exists.
     */
    public UpdateRequestBuilder setUpsert(XContentBuilder source) {
        this.upsertSourceXContentBuilder = source;
        return this;
    }

    /**
     * Sets the doc source of the update request to be used when the document does not exists.
     */
    public UpdateRequestBuilder setUpsert(Map<String, Object> source) {
        this.upsertSourceMap = source;
        return this;
    }

    /**
     * Sets the doc source of the update request to be used when the document does not exists.
     */
    public UpdateRequestBuilder setUpsert(Map<String, Object> source, XContentType contentType) {
        this.upsertSourceMap = source;
        this.upsertSourceXContentType = contentType;
        return this;
    }

    /**
     * Sets the doc source of the update request to be used when the document does not exists.
     */
    public UpdateRequestBuilder setUpsert(String source, XContentType xContentType) {
        this.upsertSourceString = source;
        this.upsertSourceXContentType = xContentType;
        return this;
    }

    /**
     * Sets the doc source of the update request to be used when the document does not exists.
     */
    public UpdateRequestBuilder setUpsert(byte[] source, XContentType xContentType) {
        this.upsertSourceBytes = source;
        this.upsertSourceXContentType = xContentType;
        return this;
    }

    /**
     * Sets the doc source of the update request to be used when the document does not exists.
     */
    public UpdateRequestBuilder setUpsert(byte[] source, int offset, int length, XContentType xContentType) {
        this.upsertSourceBytes = source;
        this.upsertSourceOffset = offset;
        this.upsertSourceLength = length;
        this.upsertSourceXContentType = xContentType;
        return this;
    }

    /**
     * Sets the doc source of the update request to be used when the document does not exists. The doc
     * includes field and value pairs.
     */
    public UpdateRequestBuilder setUpsert(Object... source) {
        this.upsertSourceArray = source;
        return this;
    }

    /**
     * Sets the doc source of the update request to be used when the document does not exists. The doc
     * includes field and value pairs.
     */
    public UpdateRequestBuilder setUpsert(XContentType xContentType, Object... source) {
        this.upsertSourceArray = source;
        this.upsertSourceXContentType = xContentType;
        return this;
    }

    /**
     * Sets whether the specified doc parameter should be used as upsert document.
     */
    public UpdateRequestBuilder setDocAsUpsert(boolean shouldUpsertDoc) {
        this.docAsUpsert = shouldUpsertDoc;
        return this;
    }

    /**
     * Sets whether to perform extra effort to detect noop updates via docAsUpsert.
     * Defaults to true.
     */
    public UpdateRequestBuilder setDetectNoop(boolean detectNoop) {
        this.detectNoop = detectNoop;
        return this;
    }

    /**
     * Sets whether the script should be run in the case of an insert
     */
    public UpdateRequestBuilder setScriptedUpsert(boolean scriptedUpsert) {
        this.scriptedUpsert = scriptedUpsert;
        return this;
    }

    /**
     * Sets the require_alias flag
     */
    public UpdateRequestBuilder setRequireAlias(boolean requireAlias) {
        this.requireAlias = requireAlias;
        return this;
    }

    @Override
    public UpdateRequestBuilder setRefreshPolicy(WriteRequest.RefreshPolicy refreshPolicy) {
        this.refreshPolicy = refreshPolicy;
        return this;
    }

    @Override
    public UpdateRequestBuilder setRefreshPolicy(String refreshPolicy) {
        this.refreshPolicyString = refreshPolicy;
        return this;
    }

    @Override
    public UpdateRequest request() {
        validate();
        UpdateRequest request = new UpdateRequest();
        super.apply(request);
        if (id != null) {
            request.id(id);
        }
        if (routing != null) {
            request.routing(routing);
        }
        if (script != null) {
            request.script(script);
        }
        if (fetchSourceInclude != null || fetchSourceExclude != null) {
            request.fetchSource(fetchSourceInclude, fetchSourceExclude);
        }
        if (fetchSourceIncludeArray != null || fetchSourceExcludeArray != null) {
            request.fetchSource(fetchSourceIncludeArray, fetchSourceExcludeArray);
        }
        if (fetchSource != null) {
            request.fetchSource(fetchSource);
        }
        if (retryOnConflict != null) {
            request.retryOnConflict(retryOnConflict);
        }
        if (version != null) {
            request.version(version);
        }
        if (versionType != null) {
            request.versionType(versionType);
        }
        if (ifSeqNo != null) {
            request.setIfSeqNo(ifSeqNo);
        }
        if (ifPrimaryTerm != null) {
            request.setIfPrimaryTerm(ifPrimaryTerm);
        }
        if (waitForActiveShards != null) {
            request.waitForActiveShards(waitForActiveShards);
        }
        if (doc != null) {
            request.doc(doc);
        }
        if (docSourceXContentBuilder != null) {
            request.doc(docSourceXContentBuilder);
        }
        if (docSourceMap != null) {
            if (docSourceXContentType == null) {
                request.doc(docSourceMap);
            } else {
                request.doc(docSourceMap, docSourceXContentType);
            }
        }
        if (docSourceString != null && docSourceXContentType != null) {
            request.doc(docSourceString, docSourceXContentType);
        }
        if (docSourceBytes != null && docSourceXContentType != null) {
            if (docSourceOffset != null && docSourceLength != null) {
                request.doc(docSourceBytes, docSourceOffset, docSourceLength, docSourceXContentType);
            }
        }
        if (docSourceArray != null) {
            if (docSourceXContentType == null) {
                request.doc(docSourceArray);
            } else {
                request.doc(docSourceXContentType, docSourceArray);
            }
        }
        if (upsert != null) {
            request.upsert(upsert);
        }
        if (upsertSourceXContentBuilder != null) {
            request.upsert(upsertSourceXContentBuilder);
        }
        if (upsertSourceMap != null) {
            if (upsertSourceXContentType == null) {
                request.upsert(upsertSourceMap);
            } else {
                request.upsert(upsertSourceMap, upsertSourceXContentType);
            }
        }
        if (upsertSourceString != null && upsertSourceXContentType != null) {
            request.upsert(upsertSourceString, upsertSourceXContentType);
        }
        if (upsertSourceBytes != null && upsertSourceXContentType != null) {
            if (upsertSourceOffset != null && upsertSourceLength != null) {
                request.upsert(upsertSourceBytes, upsertSourceOffset, upsertSourceLength, upsertSourceXContentType);
            }
        }
        if (upsertSourceArray != null) {
            if (upsertSourceXContentType == null) {
                request.upsert(upsertSourceArray);
            } else {
                request.upsert(upsertSourceXContentType, upsertSourceArray);
            }
        }
        if (docAsUpsert != null) {
            request.docAsUpsert(docAsUpsert);
        }
        if (detectNoop != null) {
            request.detectNoop(detectNoop);
        }
        if (scriptedUpsert != null) {
            request.scriptedUpsert(scriptedUpsert);
        }
        if (requireAlias != null) {
            request.setRequireAlias(requireAlias);
        }
        if (refreshPolicy != null) {
            request.setRefreshPolicy(refreshPolicy);
        }
        if (refreshPolicyString != null) {
            request.setRefreshPolicy(refreshPolicyString);
        }
        return request;
    }

    @Override
    protected void validate() throws IllegalStateException {
        super.validate();
        boolean fetchIncludeExcludeNotNull = fetchSourceInclude != null || fetchSourceExclude != null;
        boolean fetchIncludeExcludeArrayNotNull = fetchSourceIncludeArray != null || fetchSourceExcludeArray != null;
        boolean fetchSourceNotNull = fetchSource != null;
        if ((fetchIncludeExcludeNotNull && fetchIncludeExcludeArrayNotNull)
            || (fetchIncludeExcludeNotNull && fetchSourceNotNull)
            || (fetchIncludeExcludeArrayNotNull && fetchSourceNotNull)) {
            throw new IllegalStateException("Only one fetchSource() method may be called");
        }
        int docSourceFieldsSet = countDocSourceFieldsSet();
        if (docSourceFieldsSet > 1) {
            throw new IllegalStateException("Only one setDoc() method may be called, but " + docSourceFieldsSet + " have been");
        }
        int upsertSourceFieldsSet = countUpsertSourceFieldsSet();
        if (upsertSourceFieldsSet > 1) {
            throw new IllegalStateException("Only one setUpsert() method may be called, but " + upsertSourceFieldsSet + " have been");
        }
    }

    private int countDocSourceFieldsSet() {
        return countNonNullObjects(doc, docSourceXContentBuilder, docSourceMap, docSourceString, docSourceBytes, docSourceArray);
    }

    private int countUpsertSourceFieldsSet() {
        return countNonNullObjects(
            upsert,
            upsertSourceXContentBuilder,
            upsertSourceMap,
            upsertSourceString,
            upsertSourceBytes,
            upsertSourceArray
        );
    }

    private int countNonNullObjects(Object... objects) {
        int sum = 0;
        for (Object object : objects) {
            if (object != null) {
                sum++;
            }
        }
        return sum;
    }
}
