/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.client.security;

import org.elasticsearch.client.security.user.User;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;

import java.io.IOException;
import java.util.List;
import java.util.Map;
import java.util.Objects;

import static org.elasticsearch.common.xcontent.ConstructingObjectParser.constructorArg;
import static org.elasticsearch.common.xcontent.ConstructingObjectParser.optionalConstructorArg;

/**
 * The response for the authenticate call. The response contains two fields: a
 * user field and a boolean flag signaling if the user is enabled or not. The
 * user object contains all user metadata which Elasticsearch uses to map roles,
 * etc.
 */
public final class AuthenticateResponse implements ToXContentObject {

    static final ParseField USERNAME = new ParseField("username");
    static final ParseField ROLES = new ParseField("roles");
    static final ParseField METADATA = new ParseField("metadata");
    static final ParseField FULL_NAME = new ParseField("full_name");
    static final ParseField EMAIL = new ParseField("email");
    static final ParseField ENABLED = new ParseField("enabled");
    static final ParseField AUTHENTICATION_REALM = new ParseField("authentication_realm");
    static final ParseField LOOKUP_REALM = new ParseField("lookup_realm");
    static final ParseField REALM_NAME = new ParseField("name");
    static final ParseField REALM_TYPE = new ParseField("type");
    static final ParseField AUTHENTICATION_TYPE = new ParseField("authentication_type");

    @SuppressWarnings("unchecked")
    private static final ConstructingObjectParser<AuthenticateResponse, Void> PARSER = new ConstructingObjectParser<>(
            "client_security_authenticate_response", true,
            a -> new AuthenticateResponse(new User((String) a[0], ((List<String>) a[1]), (Map<String, Object>) a[2],
                (String) a[3], (String) a[4]), (Boolean) a[5], (RealmInfo) a[6], (RealmInfo) a[7], (String) a[8]));
    static {
        final ConstructingObjectParser<RealmInfo, Void> realmInfoParser = new ConstructingObjectParser<>("realm_info", true,
            a -> new RealmInfo((String) a[0], (String) a[1]));
        realmInfoParser.declareString(constructorArg(), REALM_NAME);
        realmInfoParser.declareString(constructorArg(), REALM_TYPE);
        PARSER.declareString(constructorArg(), USERNAME);
        PARSER.declareStringArray(constructorArg(), ROLES);
        PARSER.<Map<String, Object>>declareObject(constructorArg(), (parser, c) -> parser.map(), METADATA);
        PARSER.declareStringOrNull(optionalConstructorArg(), FULL_NAME);
        PARSER.declareStringOrNull(optionalConstructorArg(), EMAIL);
        PARSER.declareBoolean(constructorArg(), ENABLED);
        PARSER.declareObject(constructorArg(), realmInfoParser, AUTHENTICATION_REALM);
        PARSER.declareObject(constructorArg(), realmInfoParser, LOOKUP_REALM);
        PARSER.declareString(constructorArg(), AUTHENTICATION_TYPE);
    }

    private final User user;
    private final boolean enabled;
    private final RealmInfo authenticationRealm;
    private final RealmInfo lookupRealm;
    private final String authenticationType;


    public AuthenticateResponse(User user, boolean enabled, RealmInfo authenticationRealm,
                                RealmInfo lookupRealm, String authenticationType) {
        this.user = user;
        this.enabled = enabled;
        this.authenticationRealm = authenticationRealm;
        this.lookupRealm = lookupRealm;
        this.authenticationType = authenticationType;
    }

    /**
     * @return The effective user. This is the authenticated user, or, when
     *         submitting requests on behalf of other users, it is the
     *         impersonated user.
     */
    public User getUser() {
        return user;
    }

    /**
     * @return whether the user is enabled or not
     */
    public boolean enabled() {
        return enabled;
    }

    /**
     * @return the realm that authenticated the user
     */
    public RealmInfo getAuthenticationRealm() {
        return authenticationRealm;
    }

    /**
     * @return the realm where the user information was looked up
     */
    public RealmInfo getLookupRealm() {
        return lookupRealm;
    }

    public String getAuthenticationType() {
        return authenticationType;
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject()
            .field("username", user.getUsername())
            .field("roles", user.getRoles())
            .field("metadata", user.getMetadata())
            .field("full_name", user.getFullName())
            .field("email", user.getEmail())
            .field("enabled", enabled);
        builder.startObject("authentication_realm")
            .field("name", authenticationRealm.name)
            .field("type", authenticationRealm.type);
        builder.endObject();
        builder.startObject("lookup_realm")
            .field("name", lookupRealm == null? authenticationRealm.name: lookupRealm.name)
            .field("type", lookupRealm == null? authenticationRealm.type: lookupRealm.type);
        builder.endObject();
            builder.field("authentication_type", authenticationType);
        return builder.endObject();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        AuthenticateResponse that = (AuthenticateResponse) o;
        return enabled == that.enabled &&
            Objects.equals(user, that.user) &&
            Objects.equals(authenticationRealm, that.authenticationRealm) &&
            Objects.equals(lookupRealm, that.lookupRealm) &&
            Objects.equals(authenticationType, that.authenticationType);
    }

    @Override
    public int hashCode() {
        return Objects.hash(user, enabled, authenticationRealm, lookupRealm, authenticationType);
    }

    public static AuthenticateResponse fromXContent(XContentParser parser) throws IOException {
        return PARSER.parse(parser, null);
    }

    public static class RealmInfo {
        private String name;
        private String type;

        RealmInfo(String name, String type) {
            this.name = name;
            this.type = type;
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
            RealmInfo realmInfo = (RealmInfo) o;
            return Objects.equals(name, realmInfo.name) &&
                Objects.equals(type, realmInfo.type);
        }

        @Override
        public int hashCode() {
            return Objects.hash(name, type);
        }
    }
}
