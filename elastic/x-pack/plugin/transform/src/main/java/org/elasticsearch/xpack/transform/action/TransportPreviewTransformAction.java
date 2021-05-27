/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.transform.action;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ingest.SimulatePipelineAction;
import org.elasticsearch.action.ingest.SimulatePipelineRequest;
import org.elasticsearch.action.ingest.SimulatePipelineResponse;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.logging.HeaderWarning;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.ingest.IngestService;
import org.elasticsearch.license.License;
import org.elasticsearch.license.RemoteClusterLicenseChecker;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ClientHelper;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.common.validation.SourceDestValidator;
import org.elasticsearch.xpack.core.security.SecurityContext;
import org.elasticsearch.xpack.core.transform.TransformField;
import org.elasticsearch.xpack.core.transform.action.PreviewTransformAction;
import org.elasticsearch.xpack.core.transform.action.PreviewTransformAction.Request;
import org.elasticsearch.xpack.core.transform.action.PreviewTransformAction.Response;
import org.elasticsearch.xpack.core.transform.transforms.SourceConfig;
import org.elasticsearch.xpack.core.transform.transforms.SyncConfig;
import org.elasticsearch.xpack.core.transform.transforms.TransformConfig;
import org.elasticsearch.xpack.core.transform.transforms.TransformDestIndexSettings;
import org.elasticsearch.xpack.transform.persistence.TransformIndex;
import org.elasticsearch.xpack.transform.transforms.Function;
import org.elasticsearch.xpack.transform.transforms.FunctionFactory;
import org.elasticsearch.xpack.transform.transforms.TransformNodes;
import org.elasticsearch.xpack.transform.utils.SourceDestValidations;

import java.time.Clock;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.xpack.core.transform.action.PreviewTransformAction.DUMMY_DEST_INDEX_FOR_PREVIEW;

public class TransportPreviewTransformAction extends HandledTransportAction<Request, Response> {

    private static final int NUMBER_OF_PREVIEW_BUCKETS = 100;
    private final XPackLicenseState licenseState;
    private final SecurityContext securityContext;
    private final IndexNameExpressionResolver indexNameExpressionResolver;
    private final Client client;
    private final ThreadPool threadPool;
    private final ClusterService clusterService;
    private final TransportService transportService;
    private final Settings nodeSettings;
    private final SourceDestValidator sourceDestValidator;

    @Inject
    public TransportPreviewTransformAction(
        XPackLicenseState licenseState,
        TransportService transportService,
        ActionFilters actionFilters,
        Client client,
        ThreadPool threadPool,
        IndexNameExpressionResolver indexNameExpressionResolver,
        ClusterService clusterService,
        Settings settings,
        IngestService ingestService
    ) {
        this(
            PreviewTransformAction.NAME,
            licenseState,
            transportService,
            actionFilters,
            client,
            threadPool,
            indexNameExpressionResolver,
            clusterService,
            settings,
            ingestService
        );
    }

    protected TransportPreviewTransformAction(
        String name,
        XPackLicenseState licenseState,
        TransportService transportService,
        ActionFilters actionFilters,
        Client client,
        ThreadPool threadPool,
        IndexNameExpressionResolver indexNameExpressionResolver,
        ClusterService clusterService,
        Settings settings,
        IngestService ingestService
    ) {
        super(name, transportService, actionFilters, Request::new);
        this.licenseState = licenseState;
        this.securityContext = XPackSettings.SECURITY_ENABLED.get(settings)
            ? new SecurityContext(settings, threadPool.getThreadContext())
            : null;
        this.indexNameExpressionResolver = indexNameExpressionResolver;
        this.client = client;
        this.threadPool = threadPool;
        this.clusterService = clusterService;
        this.transportService = transportService;
        this.nodeSettings = settings;
        this.sourceDestValidator = new SourceDestValidator(
            indexNameExpressionResolver,
            transportService.getRemoteClusterService(),
            DiscoveryNode.isRemoteClusterClient(settings)
                /* transforms are BASIC so always allowed, no need to check license */
                ? new RemoteClusterLicenseChecker(client, mode -> true) : null,
            ingestService,
            clusterService.getNodeName(),
            License.OperationMode.BASIC.description()
        );
    }

    @Override
    protected void doExecute(Task task, Request request, ActionListener<Response> listener) {
        final ClusterState clusterState = clusterService.state();
        TransformNodes.throwIfNoTransformNodes(clusterState);

        // Redirection can only be performed between nodes that are at least 7.13.
        if (clusterState.nodes().getMinNodeVersion().onOrAfter(Version.V_7_13_0)) {
            boolean requiresRemote = request.getConfig().getSource().requiresRemoteCluster();
            if (TransformNodes.redirectToAnotherNodeIfNeeded(
                    clusterState, nodeSettings, requiresRemote, transportService, actionName, request, Response::new, listener)) {
                return;
            }
        }

        final TransformConfig config = request.getConfig();
        final Function function = FunctionFactory.create(config);

        // <4> Validate transform query
        ActionListener<Boolean> validateConfigListener = ActionListener.wrap(
            validateConfigResponse -> {
                getPreview(
                    config.getId(), // note: @link{PreviewTransformAction} sets an id, so this is never null
                    function,
                    config.getSource(),
                    config.getDestination().getPipeline(),
                    config.getDestination().getIndex(),
                    config.getSyncConfig(),
                    listener
                );
            },
            listener::onFailure
        );

        // <3> Validate transform function config
        ActionListener<Boolean> validateSourceDestListener = ActionListener.wrap(
            validateSourceDestResponse -> {
                function.validateConfig(validateConfigListener);
            },
            listener::onFailure
        );

        // <2> Validate source and destination indices
        ActionListener<Void> checkPrivilegesListener = ActionListener.wrap(
            aVoid -> {
                sourceDestValidator.validate(
                    clusterState,
                    config.getSource().getIndex(),
                    config.getDestination().getIndex(),
                    config.getDestination().getPipeline(),
                    SourceDestValidations.getValidationsForPreview(config.getAdditionalSourceDestValidations()),
                    validateSourceDestListener
                );
            },
            listener::onFailure
        );

        // <1> Early check to verify that the user can create the destination index and can read from the source
        if (licenseState.isSecurityEnabled()) {
            TransformPrivilegeChecker.checkPrivileges(
                "preview",
                securityContext,
                indexNameExpressionResolver,
                clusterState,
                client,
                config,
                // We don't want to check privileges for a dummy (placeholder) index and the placeholder is inserted as config.dest.index
                // early in the REST action so the only possibility we have here is string comparison.
                DUMMY_DEST_INDEX_FOR_PREVIEW.equals(config.getDestination().getIndex()) == false,
                checkPrivilegesListener
            );
        } else { // No security enabled, just create the transform
            checkPrivilegesListener.onResponse(null);
        }
    }

    @SuppressWarnings("unchecked")
    private void getPreview(
        String transformId,
        Function function,
        SourceConfig source,
        String pipeline,
        String dest,
        SyncConfig syncConfig,
        ActionListener<Response> listener
    ) {
        final SetOnce<Map<String, String>> mappings = new SetOnce<>();

        ActionListener<SimulatePipelineResponse> pipelineResponseActionListener = ActionListener.wrap(simulatePipelineResponse -> {
            List<Map<String, Object>> docs = new ArrayList<>(simulatePipelineResponse.getResults().size());
            for (var simulateDocumentResult : simulatePipelineResponse.getResults()) {
                try (XContentBuilder xContentBuilder = XContentFactory.jsonBuilder()) {
                    XContentBuilder content = simulateDocumentResult.toXContent(xContentBuilder, ToXContent.EMPTY_PARAMS);
                    Map<String, Object> tempMap = XContentHelper.convertToMap(BytesReference.bytes(content), true, XContentType.JSON).v2();
                    docs.add((Map<String, Object>) XContentMapValues.extractValue("doc._source", tempMap));
                }
            }
            TransformDestIndexSettings generatedDestIndexSettings = TransformIndex.createTransformDestIndexSettings(
                mappings.get(),
                transformId,
                Clock.systemUTC()
            );

            List<String> warnings = TransformConfigLinter.getWarnings(function, source, syncConfig);
            warnings.forEach(HeaderWarning::addWarning);
            listener.onResponse(new Response(docs, generatedDestIndexSettings));
        }, listener::onFailure);

        ActionListener<List<Map<String, Object>>> previewListener = ActionListener.wrap(
            docs -> {
                if (pipeline == null) {
                    TransformDestIndexSettings generatedDestIndexSettings = TransformIndex.createTransformDestIndexSettings(
                        mappings.get(),
                        transformId,
                        Clock.systemUTC()
                    );
                    List<String> warnings = TransformConfigLinter.getWarnings(function, source, syncConfig);
                    warnings.forEach(HeaderWarning::addWarning);
                    listener.onResponse(new Response(docs, generatedDestIndexSettings));
                } else {
                    List<Map<String, Object>> results = docs.stream().map(doc -> {
                        Map<String, Object> src = new HashMap<>();
                        String id = (String) doc.get(TransformField.DOCUMENT_ID_FIELD);
                        src.put("_source", doc);
                        src.put("_id", id);
                        src.put("_index", dest);
                        return src;
                    }).collect(Collectors.toList());

                    try (XContentBuilder builder = jsonBuilder()) {
                        builder.startObject();
                        builder.field("docs", results);
                        builder.endObject();
                        var pipelineRequest = new SimulatePipelineRequest(BytesReference.bytes(builder), XContentType.JSON);
                        pipelineRequest.setId(pipeline);
                        client.execute(SimulatePipelineAction.INSTANCE, pipelineRequest, pipelineResponseActionListener);
                    }
                }
            },
            listener::onFailure
        );

        ActionListener<Map<String, String>> deduceMappingsListener = ActionListener.wrap(
            deducedMappings -> {
                mappings.set(deducedMappings);
                function.preview(
                    client,
                    ClientHelper.filterSecurityHeaders(threadPool.getThreadContext().getHeaders()),
                    source,
                    deducedMappings,
                    NUMBER_OF_PREVIEW_BUCKETS,
                    previewListener
                );
            },
            listener::onFailure
        );

        function.deduceMappings(client, source, deduceMappingsListener);
    }
}
