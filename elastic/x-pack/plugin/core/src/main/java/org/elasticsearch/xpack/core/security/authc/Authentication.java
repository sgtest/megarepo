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
import org.elasticsearch.xpack.core.security.user.InternalUserSerializationHelper;
import org.elasticsearch.xpack.core.security.user.User;

import java.io.IOException;
import java.util.Base64;
import java.util.Objects;

// TODO(hub-cap) Clean this up after moving User over - This class can re-inherit its field AUTHENTICATION_KEY in AuthenticationField.
// That interface can be removed
public class Authentication {

    private final User user;
    private final RealmRef authenticatedBy;
    private final RealmRef lookedUpBy;
    private final Version version;

    public Authentication(User user, RealmRef authenticatedBy, RealmRef lookedUpBy) {
        this(user, authenticatedBy, lookedUpBy, Version.CURRENT);
    }

    public Authentication(User user, RealmRef authenticatedBy, RealmRef lookedUpBy, Version version) {
        this.user = Objects.requireNonNull(user);
        this.authenticatedBy = Objects.requireNonNull(authenticatedBy);
        this.lookedUpBy = lookedUpBy;
        this.version = version;
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

    public Version getVersion() {
        return version;
    }

    public static Authentication readFromContext(ThreadContext ctx)
            throws IOException, IllegalArgumentException {
        Authentication authentication = ctx.getTransient(AuthenticationField.AUTHENTICATION_KEY);
        if (authentication != null) {
            assert ctx.getHeader(AuthenticationField.AUTHENTICATION_KEY) != null;
            return authentication;
        }

        String authenticationHeader = ctx.getHeader(AuthenticationField.AUTHENTICATION_KEY);
        if (authenticationHeader == null) {
            return null;
        }
        return deserializeHeaderAndPutInContext(authenticationHeader, ctx);
    }

    public static Authentication getAuthentication(ThreadContext context) {
        return context.getTransient(AuthenticationField.AUTHENTICATION_KEY);
    }

    static Authentication deserializeHeaderAndPutInContext(String header, ThreadContext ctx)
            throws IOException, IllegalArgumentException {
        assert ctx.getTransient(AuthenticationField.AUTHENTICATION_KEY) == null;

        byte[] bytes = Base64.getDecoder().decode(header);
        StreamInput input = StreamInput.wrap(bytes);
        Version version = Version.readVersion(input);
        input.setVersion(version);
        Authentication authentication = new Authentication(input);
        ctx.putTransient(AuthenticationField.AUTHENTICATION_KEY, authentication);
        return authentication;
    }

    /**
     * Writes the authentication to the context. There must not be an existing authentication in the context and if there is an
     * {@link IllegalStateException} will be thrown
     */
    public void writeToContext(ThreadContext ctx)
            throws IOException, IllegalArgumentException {
        ensureContextDoesNotContainAuthentication(ctx);
        String header = encode();
        ctx.putTransient(AuthenticationField.AUTHENTICATION_KEY, this);
        ctx.putHeader(AuthenticationField.AUTHENTICATION_KEY, header);
    }

    void ensureContextDoesNotContainAuthentication(ThreadContext ctx) {
        if (ctx.getTransient(AuthenticationField.AUTHENTICATION_KEY) != null) {
            if (ctx.getHeader(AuthenticationField.AUTHENTICATION_KEY) == null) {
                throw new IllegalStateException("authentication present as a transient but not a header");
            }
            throw new IllegalStateException("authentication is already present in the context");
        }
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
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;

        Authentication that = (Authentication) o;

        if (!user.equals(that.user)) return false;
        if (!authenticatedBy.equals(that.authenticatedBy)) return false;
        if (lookedUpBy != null ? !lookedUpBy.equals(that.lookedUpBy) : that.lookedUpBy != null) return false;
        return version.equals(that.version);
    }

    @Override
    public int hashCode() {
        int result = user.hashCode();
        result = 31 * result + authenticatedBy.hashCode();
        result = 31 * result + (lookedUpBy != null ? lookedUpBy.hashCode() : 0);
        result = 31 * result + version.hashCode();
        return result;
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
    }
}

