/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.action.token;

import org.elasticsearch.Version;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.Objects;

/**
 * Response containing the token string that was generated from a token creation request. This
 * object also contains the scope and expiration date. If the scope was not provided or if the
 * provided scope matches the scope of the token, then the scope value is <code>null</code>
 */
public final class CreateTokenResponse extends ActionResponse implements ToXContentObject {

    private String tokenString;
    private TimeValue expiresIn;
    private String scope;
    private String refreshToken;

    CreateTokenResponse() {}

    public CreateTokenResponse(String tokenString, TimeValue expiresIn, String scope, String refreshToken) {
        this.tokenString = Objects.requireNonNull(tokenString);
        this.expiresIn = Objects.requireNonNull(expiresIn);
        this.scope = scope;
        this.refreshToken = refreshToken;
    }

    public String getTokenString() {
        return tokenString;
    }

    public String getScope() {
        return scope;
    }

    public TimeValue getExpiresIn() {
        return expiresIn;
    }

    public String getRefreshToken() {
        return refreshToken;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        super.writeTo(out);
        out.writeString(tokenString);
        out.writeTimeValue(expiresIn);
        out.writeOptionalString(scope);
        if (out.getVersion().onOrAfter(Version.V_6_2_0)) {
            out.writeString(refreshToken);
        }
    }

    @Override
    public void readFrom(StreamInput in) throws IOException {
        super.readFrom(in);
        tokenString = in.readString();
        expiresIn = in.readTimeValue();
        scope = in.readOptionalString();
        if (in.getVersion().onOrAfter(Version.V_6_2_0)) {
            refreshToken = in.readString();
        }
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject()
            .field("access_token", tokenString)
            .field("type", "Bearer")
            .field("expires_in", expiresIn.seconds());
        if (refreshToken != null) {
            builder.field("refresh_token", refreshToken);
        }
        // only show the scope if it is not null
        if (scope != null) {
            builder.field("scope", scope);
        }
        return builder.endObject();
    }
}
