/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.authc;

import org.elasticsearch.Version;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.xpack.core.security.authc.support.AuthenticationContextSerializer;
import org.elasticsearch.xpack.core.security.user.InternalUserSerializationHelper;
import org.elasticsearch.xpack.core.security.user.User;

import java.io.IOException;
import java.util.Base64;
import java.util.Collections;
import java.util.Map;
import java.util.Objects;

// TODO(hub-cap) Clean this up after moving User over - This class can re-inherit its field AUTHENTICATION_KEY in AuthenticationField.
// That interface can be removed
public class Authentication implements ToXContentObject {

    public static final Version VERSION_API_KEY_ROLES_AS_BYTES = Version.V_7_9_0;

    private final User user;
    private final RealmRef authenticatedBy;
    private final RealmRef lookedUpBy;
    private final Version version;
    private final AuthenticationType type;
    private final Map<String, Object> metadata;

    public Authentication(User user, RealmRef authenticatedBy, RealmRef lookedUpBy) {
        this(user, authenticatedBy, lookedUpBy, Version.CURRENT);
    }

    public Authentication(User user, RealmRef authenticatedBy, RealmRef lookedUpBy, Version version) {
        this(user, authenticatedBy, lookedUpBy, version, AuthenticationType.REALM, Collections.emptyMap());
    }

    public Authentication(User user, RealmRef authenticatedBy, RealmRef lookedUpBy, Version version,
                          AuthenticationType type, Map<String, Object> metadata) {
        this.user = Objects.requireNonNull(user);
        this.authenticatedBy = Objects.requireNonNull(authenticatedBy);
        this.lookedUpBy = lookedUpBy;
        this.version = version;
        this.type = type;
        this.metadata = metadata;
    }

    public Authentication(StreamInput in) throws IOException {
        this.user = InternalUserSerializationHelper.readFrom(in);
        this.authenticatedBy = new RealmRef(in);
        if (in.readBoolean()) {
            this.lookedUpBy = new RealmRef(in);
        } else {
            this.lookedUpBy = null;
        }
        this.version = in.getVersion();
        type = AuthenticationType.values()[in.readVInt()];
        metadata = in.readMap();
    }

    public User getUser() {
        return user;
    }

    public RealmRef getAuthenticatedBy() {
        return authenticatedBy;
    }

    public RealmRef getLookedUpBy() {
        return lookedUpBy;
    }

    /**
     * Get the realm where the effective user comes from.
     * The effective user is the es-security-runas-user if present or the authenticated user.
     */
    public RealmRef getSourceRealm() {
        return lookedUpBy == null ? authenticatedBy : lookedUpBy;
    }

    public Version getVersion() {
        return version;
    }

    public AuthenticationType getAuthenticationType() {
        return type;
    }

    public Map<String, Object> getMetadata() {
        return metadata;
    }

    /**
     * Writes the authentication to the context. There must not be an existing authentication in the context and if there is an
     * {@link IllegalStateException} will be thrown
     */
    public void writeToContext(ThreadContext ctx) throws IOException, IllegalArgumentException {
        new AuthenticationContextSerializer().writeToContext(this, ctx);
    }

    public String encode() throws IOException {
        BytesStreamOutput output = new BytesStreamOutput();
        output.setVersion(version);
        Version.writeVersion(version, output);
        writeTo(output);
        return Base64.getEncoder().encodeToString(BytesReference.toBytes(output.bytes()));
    }

    public void writeTo(StreamOutput out) throws IOException {
        InternalUserSerializationHelper.writeTo(user, out);
        authenticatedBy.writeTo(out);
        if (lookedUpBy != null) {
            out.writeBoolean(true);
            lookedUpBy.writeTo(out);
        } else {
            out.writeBoolean(false);
        }
        out.writeVInt(type.ordinal());
        out.writeMap(metadata);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        Authentication that = (Authentication) o;
        return user.equals(that.user) &&
            authenticatedBy.equals(that.authenticatedBy) &&
            Objects.equals(lookedUpBy, that.lookedUpBy) &&
            version.equals(that.version) &&
            type == that.type &&
            metadata.equals(that.metadata);
    }

    @Override
    public int hashCode() {
        return Objects.hash(user, authenticatedBy, lookedUpBy, version, type, metadata);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        toXContentFragment(builder);
        return builder.endObject();
    }

    /**
     * Generates XContent without the start/end object.
     */
    public void toXContentFragment(XContentBuilder builder) throws IOException {
        builder.field(User.Fields.USERNAME.getPreferredName(), user.principal());
        builder.array(User.Fields.ROLES.getPreferredName(), user.roles());
        builder.field(User.Fields.FULL_NAME.getPreferredName(), user.fullName());
        builder.field(User.Fields.EMAIL.getPreferredName(), user.email());
        builder.field(User.Fields.METADATA.getPreferredName(), user.metadata());
        builder.field(User.Fields.ENABLED.getPreferredName(), user.enabled());
        builder.startObject(User.Fields.AUTHENTICATION_REALM.getPreferredName());
        builder.field(User.Fields.REALM_NAME.getPreferredName(), getAuthenticatedBy().getName());
        builder.field(User.Fields.REALM_TYPE.getPreferredName(), getAuthenticatedBy().getType());
        builder.endObject();
        builder.startObject(User.Fields.LOOKUP_REALM.getPreferredName());
        if (getLookedUpBy() != null) {
            builder.field(User.Fields.REALM_NAME.getPreferredName(), getLookedUpBy().getName());
            builder.field(User.Fields.REALM_TYPE.getPreferredName(), getLookedUpBy().getType());
        } else {
            builder.field(User.Fields.REALM_NAME.getPreferredName(), getAuthenticatedBy().getName());
            builder.field(User.Fields.REALM_TYPE.getPreferredName(), getAuthenticatedBy().getType());
        }
        builder.endObject();
    }

    @Override
    public String toString() {
        StringBuilder builder = new StringBuilder("Authentication[")
            .append(user)
            .append(",type=").append(type)
            .append(",by=").append(authenticatedBy);
        if (lookedUpBy != null) {
            builder.append(",lookup=").append(lookedUpBy);
        }
        builder.append("]");
        return builder.toString();
    }

    public static class RealmRef {

        private final String nodeName;
        private final String name;
        private final String type;

        public RealmRef(String name, String type, String nodeName) {
            this.nodeName = nodeName;
            this.name = name;
            this.type = type;
        }

        public RealmRef(StreamInput in) throws IOException {
            this.nodeName = in.readString();
            this.name = in.readString();
            this.type = in.readString();
        }

        void writeTo(StreamOutput out) throws IOException {
            out.writeString(nodeName);
            out.writeString(name);
            out.writeString(type);
        }

        public String getNodeName() {
            return nodeName;
        }

        public String getName() {
            return name;
        }

        public String getType() {
            return type;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;

            RealmRef realmRef = (RealmRef) o;

            if (!nodeName.equals(realmRef.nodeName)) return false;
            if (!name.equals(realmRef.name)) return false;
            return type.equals(realmRef.type);
        }

        @Override
        public int hashCode() {
            int result = nodeName.hashCode();
            result = 31 * result + name.hashCode();
            result = 31 * result + type.hashCode();
            return result;
        }

        @Override
        public String toString() {
            return "{Realm[" + type + "." + name + "] on Node[" + nodeName + "]}";
        }
    }

    public enum AuthenticationType {
        REALM,
        API_KEY,
        TOKEN,
        ANONYMOUS,
        INTERNAL
    }
}

