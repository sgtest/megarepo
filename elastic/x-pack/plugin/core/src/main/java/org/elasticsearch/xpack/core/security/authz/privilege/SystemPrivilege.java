/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.authz.privilege;

import org.elasticsearch.transport.TransportActionProxy;
import org.elasticsearch.xpack.core.security.support.Automatons;

import java.util.Collections;
import java.util.function.Predicate;

public final class SystemPrivilege extends Privilege {

    public static SystemPrivilege INSTANCE = new SystemPrivilege();

    private static final Predicate<String> ALLOWED_ACTIONS = Automatons.predicate(
        "internal:*",
        "indices:monitor/*", // added for monitoring
        "cluster:monitor/*",  // added for monitoring
        "cluster:admin/bootstrap/*", // for the bootstrap service
        "cluster:admin/reroute", // added for DiskThresholdDecider.DiskListener
        "indices:admin/mapping/put", // needed for recovery and shrink api
        "indices:admin/template/put", // needed for the TemplateUpgradeService
        "indices:admin/template/delete", // needed for the TemplateUpgradeService
        "indices:admin/seq_no/global_checkpoint_sync*", // needed for global checkpoint syncs
        "indices:admin/seq_no/retention_lease_sync*", // needed for retention lease syncs
        "indices:admin/settings/update" // needed for DiskThresholdMonitor.markIndicesReadOnly
    );

    private static final Predicate<String> PREDICATE = (action) -> {
        // Only allow a proxy action if the underlying action is allowed
        if (TransportActionProxy.isProxyAction(action)) {
            return ALLOWED_ACTIONS.test(TransportActionProxy.unwrapAction(action));
        } else {
            return ALLOWED_ACTIONS.test(action);
        }
    };

    private SystemPrivilege() {
        super(Collections.singleton("internal"));
    }

    @Override
    public Predicate<String> predicate() {
        return PREDICATE;
    }
}
