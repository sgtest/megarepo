/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security;

import org.elasticsearch.Version;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.xpack.core.XPackFeatureSet;
import org.elasticsearch.xpack.core.XPackField;

import java.io.IOException;
import java.util.Collections;
import java.util.Map;

public class SecurityFeatureSetUsage extends XPackFeatureSet.Usage {

    private static final String REALMS_XFIELD = "realms";
    private static final String ROLES_XFIELD = "roles";
    private static final String ROLE_MAPPING_XFIELD = "role_mapping";
    private static final String SSL_XFIELD = "ssl";
    private static final String AUDIT_XFIELD = "audit";
    private static final String IP_FILTER_XFIELD = "ipfilter";
    private static final String ANONYMOUS_XFIELD = "anonymous";

    private Map<String, Object> realmsUsage;
    private Map<String, Object> rolesStoreUsage;
    private Map<String, Object> sslUsage;
    private Map<String, Object> auditUsage;
    private Map<String, Object> ipFilterUsage;
    private Map<String, Object> anonymousUsage;
    private Map<String, Object> roleMappingStoreUsage;

    public SecurityFeatureSetUsage(StreamInput in) throws IOException {
        super(in);
        realmsUsage = in.readMap();
        rolesStoreUsage = in.readMap();
        sslUsage = in.readMap();
        auditUsage = in.readMap();
        ipFilterUsage = in.readMap();
        if (in.getVersion().before(Version.V_6_0_0_beta1)) {
            // system key has been removed but older send its usage, so read the map and ignore
            in.readMap();
        }
        anonymousUsage = in.readMap();
        roleMappingStoreUsage = in.readMap();
    }

    public SecurityFeatureSetUsage(boolean available, boolean enabled, Map<String, Object> realmsUsage,
                                   Map<String, Object> rolesStoreUsage, Map<String, Object> roleMappingStoreUsage,
                                   Map<String, Object> sslUsage, Map<String, Object> auditUsage,
                                   Map<String, Object> ipFilterUsage, Map<String, Object> anonymousUsage) {
        super(XPackField.SECURITY, available, enabled);
        this.realmsUsage = realmsUsage;
        this.rolesStoreUsage = rolesStoreUsage;
        this.roleMappingStoreUsage = roleMappingStoreUsage;
        this.sslUsage = sslUsage;
        this.auditUsage = auditUsage;
        this.ipFilterUsage = ipFilterUsage;
        this.anonymousUsage = anonymousUsage;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        super.writeTo(out);
        out.writeMap(realmsUsage);
        out.writeMap(rolesStoreUsage);
        out.writeMap(sslUsage);
        out.writeMap(auditUsage);
        out.writeMap(ipFilterUsage);
        if (out.getVersion().before(Version.V_6_0_0_beta1)) {
            // system key has been removed but older versions still expected it so send a empty map
            out.writeMap(Collections.emptyMap());
        }
        out.writeMap(anonymousUsage);
        out.writeMap(roleMappingStoreUsage);
    }

    @Override
    protected void innerXContent(XContentBuilder builder, Params params) throws IOException {
        super.innerXContent(builder, params);
        if (enabled) {
            builder.field(REALMS_XFIELD, realmsUsage);
            builder.field(ROLES_XFIELD, rolesStoreUsage);
            builder.field(ROLE_MAPPING_XFIELD, roleMappingStoreUsage);
            builder.field(SSL_XFIELD, sslUsage);
            builder.field(AUDIT_XFIELD, auditUsage);
            builder.field(IP_FILTER_XFIELD, ipFilterUsage);
            builder.field(ANONYMOUS_XFIELD, anonymousUsage);
        }
    }

    public Map<String, Object> getRealmsUsage() {
        return Collections.unmodifiableMap(realmsUsage);
    }
}
