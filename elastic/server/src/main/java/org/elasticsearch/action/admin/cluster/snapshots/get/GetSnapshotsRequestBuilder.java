/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.cluster.snapshots.get;

import org.elasticsearch.action.support.master.MasterNodeOperationRequestBuilder;
import org.elasticsearch.client.ElasticsearchClient;
import org.elasticsearch.common.util.ArrayUtils;

/**
 * Get snapshots request builder
 */
public class GetSnapshotsRequestBuilder extends MasterNodeOperationRequestBuilder<GetSnapshotsRequest,
        GetSnapshotsResponse, GetSnapshotsRequestBuilder> {

    /**
     * Constructs the new get snapshot request with specified repositories
     */
    public GetSnapshotsRequestBuilder(ElasticsearchClient client, GetSnapshotsAction action, String... repositories) {
        super(client, action, new GetSnapshotsRequest(repositories));
    }

    /**
     * Sets the repository names
     *
     * @param repositories repository names
     * @return this builder
     */
    public GetSnapshotsRequestBuilder setRepositories(String... repositories) {
        request.repositories(repositories);
        return this;
    }

    /**
     * Sets list of snapshots to return
     *
     * @param snapshots list of snapshots
     * @return this builder
     */
    public GetSnapshotsRequestBuilder setSnapshots(String... snapshots) {
        request.snapshots(snapshots);
        return this;
    }

    /**
     * Makes the request to return the current snapshot
     *
     * @return this builder
     */
    public GetSnapshotsRequestBuilder setCurrentSnapshot() {
        request.snapshots(new String[]{GetSnapshotsRequest.CURRENT_SNAPSHOT});
        return this;
    }

    /**
     * Adds additional snapshots to the list of snapshots to return
     *
     * @param snapshots additional snapshots
     * @return this builder
     */
    public GetSnapshotsRequestBuilder addSnapshots(String... snapshots) {
        request.snapshots(ArrayUtils.concat(request.snapshots(), snapshots));
        return this;
    }

    /**
     * Makes the request ignore unavailable snapshots
     *
     * @param ignoreUnavailable true to ignore unavailable snapshots.
     * @return this builder
     */
    public GetSnapshotsRequestBuilder setIgnoreUnavailable(boolean ignoreUnavailable) {
        request.ignoreUnavailable(ignoreUnavailable);
        return this;
    }

    /**
     * Set to {@code false} to only show the snapshot names and the indices they contain.
     * This is useful when the snapshots belong to a cloud-based repository where each
     * blob read is a concern (cost wise and performance wise), as the snapshot names and
     * indices they contain can be retrieved from a single index blob in the repository,
     * whereas the rest of the information requires reading a snapshot metadata file for
     * each snapshot requested.  Defaults to {@code true}, which returns all information
     * about each requested snapshot.
     */
    public GetSnapshotsRequestBuilder setVerbose(boolean verbose) {
        request.verbose(verbose);
        return this;
    }

}
