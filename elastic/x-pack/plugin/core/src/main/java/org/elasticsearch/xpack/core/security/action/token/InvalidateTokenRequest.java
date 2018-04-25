/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.action.token;

import org.elasticsearch.Version;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;

import java.io.IOException;

import static org.elasticsearch.action.ValidateActions.addValidationError;

/**
 * Request for invalidating a token so that it can no longer be used
 */
public final class InvalidateTokenRequest extends ActionRequest {

    public enum Type {
        ACCESS_TOKEN,
        REFRESH_TOKEN
    }

    private String tokenString;
    private Type tokenType;

    public InvalidateTokenRequest() {}

    /**
     * @param tokenString the string representation of the token
     */
    public InvalidateTokenRequest(String tokenString, Type type) {
        this.tokenString = tokenString;
        this.tokenType = type;
    }

    @Override
    public ActionRequestValidationException validate() {
        ActionRequestValidationException validationException = null;
        if (Strings.isNullOrEmpty(tokenString)) {
            validationException = addValidationError("token string must be provided", null);
        }
        if (tokenType == null) {
            validationException = addValidationError("token type must be provided", validationException);
        }
        return validationException;
    }

    public String getTokenString() {
        return tokenString;
    }

    void setTokenString(String token) {
        this.tokenString = token;
    }

    public Type getTokenType() {
        return tokenType;
    }

    void setTokenType(Type tokenType) {
        this.tokenType = tokenType;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        super.writeTo(out);
        out.writeString(tokenString);
        if (out.getVersion().onOrAfter(Version.V_6_2_0)) {
            out.writeVInt(tokenType.ordinal());
        } else if (tokenType == Type.REFRESH_TOKEN) {
            throw new IllegalArgumentException("refresh token invalidation cannot be serialized with version [" + out.getVersion() +
                    "]");
        }
    }

    @Override
    public void readFrom(StreamInput in) throws IOException {
        super.readFrom(in);
        tokenString = in.readString();
        if (in.getVersion().onOrAfter(Version.V_6_2_0)) {
            tokenType = Type.values()[in.readVInt()];
        } else {
            tokenType = Type.ACCESS_TOKEN;
        }
    }
}
