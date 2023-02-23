/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.security.authc;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.TransportVersion;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.util.ArrayUtils;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.xpack.core.security.authc.RemoteAccessAuthentication.RoleDescriptorsBytes;
import org.elasticsearch.xpack.core.security.authc.service.ServiceAccountSettings;
import org.elasticsearch.xpack.core.security.authz.store.RoleReference;
import org.elasticsearch.xpack.core.security.authz.store.RoleReferenceIntersection;
import org.elasticsearch.xpack.core.security.user.AnonymousUser;
import org.elasticsearch.xpack.core.security.user.User;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Objects;

import static org.elasticsearch.xpack.core.security.authc.Authentication.VERSION_API_KEY_ROLES_AS_BYTES;
import static org.elasticsearch.xpack.core.security.authc.AuthenticationField.API_KEY_LIMITED_ROLE_DESCRIPTORS_KEY;
import static org.elasticsearch.xpack.core.security.authc.AuthenticationField.API_KEY_ROLE_DESCRIPTORS_KEY;
import static org.elasticsearch.xpack.core.security.authc.AuthenticationField.REMOTE_ACCESS_AUTHENTICATION_KEY;
import static org.elasticsearch.xpack.core.security.authc.Subject.Type.API_KEY;
import static org.elasticsearch.xpack.core.security.authc.Subject.Type.REMOTE_ACCESS;

/**
 * A subject is a more generic concept similar to user and associated to the current authentication.
 * It is more generic than user because it can also represent API keys, service accounts, or remote access users.
 * It also contains authentication level information, e.g. realm and metadata so that it can answer
 * queries in a better encapsulated way.
 */
public class Subject {

    public enum Type {
        USER,
        API_KEY,
        SERVICE_ACCOUNT,
        REMOTE_ACCESS,
    }

    private final TransportVersion version;
    private final User user;
    private final Authentication.RealmRef realm;
    private final Type type;
    private final Map<String, Object> metadata;

    public Subject(User user, Authentication.RealmRef realm) {
        this(user, realm, TransportVersion.CURRENT, Map.of());
    }

    public Subject(User user, Authentication.RealmRef realm, TransportVersion version, Map<String, Object> metadata) {
        this.version = version;
        this.user = user;
        this.realm = realm;
        // Realm can be null for run-as user if it does not exist. Pretend it is a user and it will be rejected later in authorization
        // This is to be consistent with existing behaviour.
        if (realm == null) {
            this.type = Type.USER;
        } else if (AuthenticationField.API_KEY_REALM_TYPE.equals(realm.getType())) {
            assert AuthenticationField.API_KEY_REALM_NAME.equals(realm.getName()) : "api key realm name mismatch";
            this.type = Type.API_KEY;
        } else if (ServiceAccountSettings.REALM_TYPE.equals(realm.getType())) {
            assert ServiceAccountSettings.REALM_NAME.equals(realm.getName()) : "service account realm name mismatch";
            this.type = Type.SERVICE_ACCOUNT;
        } else if (AuthenticationField.REMOTE_ACCESS_REALM_TYPE.equals(realm.getType())) {
            assert AuthenticationField.REMOTE_ACCESS_REALM_NAME.equals(realm.getName()) : "remote access realm name mismatch";
            this.type = Type.REMOTE_ACCESS;
        } else {
            this.type = Type.USER;
        }
        this.metadata = metadata;
    }

    public User getUser() {
        return user;
    }

    public Authentication.RealmRef getRealm() {
        return realm;
    }

    public Type getType() {
        return type;
    }

    public Map<String, Object> getMetadata() {
        return metadata;
    }

    public TransportVersion getTransportVersion() {
        return version;
    }

    public RoleReferenceIntersection getRoleReferenceIntersection(@Nullable AnonymousUser anonymousUser) {
        switch (type) {
            case USER:
                return buildRoleReferencesForUser(anonymousUser);
            case API_KEY:
                return buildRoleReferencesForApiKey();
            case SERVICE_ACCOUNT:
                return new RoleReferenceIntersection(new RoleReference.ServiceAccountRoleReference(user.principal()));
            case REMOTE_ACCESS:
                return buildRoleReferencesForRemoteAccess();
            default:
                assert false : "unknown subject type: [" + type + "]";
                throw new IllegalStateException("unknown subject type: [" + type + "]");
        }
    }

    public boolean canAccessResourcesOf(Subject resourceCreatorSubject) {
        if (eitherIsAnApiKey(resourceCreatorSubject)) {
            if (bothAreApiKeys(resourceCreatorSubject)) {
                return isTheSameApiKey(resourceCreatorSubject);
            } else {
                // an API Key cannot access resources created by non-API Keys or vice-versa
                return false;
            }
        } else if (eitherIsRemoteAccess(resourceCreatorSubject)) {
            if (bothAreRemoteAccess(resourceCreatorSubject)) {
                if (false == isTheSameApiKey(resourceCreatorSubject)) {
                    return false;
                }
                return ((Authentication) getMetadata().get(REMOTE_ACCESS_AUTHENTICATION_KEY)).canAccessResourcesOf(
                    (Authentication) resourceCreatorSubject.getMetadata().get(REMOTE_ACCESS_AUTHENTICATION_KEY)
                );
            } else {
                // A remote access subject can never share resources with non-remote access
                return false;
            }
        } else {
            if (false == getUser().principal().equals(resourceCreatorSubject.getUser().principal())) {
                return false;
            }
            final Authentication.RealmRef myAuthRealm = getRealm();
            final Authentication.RealmRef creatorAuthRealm = resourceCreatorSubject.getRealm();
            if (null == myAuthRealm.getDomain()) {
                // the authentication accessing the resource is for a user from a realm not part of any domain
                return Authentication.equivalentRealms(
                    myAuthRealm.getName(),
                    myAuthRealm.getType(),
                    creatorAuthRealm.getName(),
                    creatorAuthRealm.getType()
                );
            } else {
                for (RealmConfig.RealmIdentifier domainRealm : myAuthRealm.getDomain().realms()) {
                    if (Authentication.equivalentRealms(
                        domainRealm.getName(),
                        domainRealm.getType(),
                        creatorAuthRealm.getName(),
                        creatorAuthRealm.getType()
                    )) {
                        return true;
                    }
                }
                return false;
            }
        }
    }

    private boolean isTheSameApiKey(Subject resourceCreatorSubject) {
        final boolean sameKeyId = getMetadata().get(AuthenticationField.API_KEY_ID_KEY)
            .equals(resourceCreatorSubject.getMetadata().get(AuthenticationField.API_KEY_ID_KEY));
        assert false == sameKeyId || getUser().principal().equals(resourceCreatorSubject.getUser().principal())
            : "The same API key ID cannot be attributed to two different usernames";
        return sameKeyId;
    }

    private boolean eitherIsAnApiKey(Subject resourceCreatorSubject) {
        return API_KEY.equals(getType()) || API_KEY.equals(resourceCreatorSubject.getType());
    }

    private boolean bothAreApiKeys(Subject resourceCreatorSubject) {
        return API_KEY.equals(getType()) && API_KEY.equals(resourceCreatorSubject.getType());
    }

    private boolean eitherIsRemoteAccess(Subject resourceCreatorSubject) {
        return REMOTE_ACCESS.equals(getType()) || REMOTE_ACCESS.equals(resourceCreatorSubject.getType());
    }

    private boolean bothAreRemoteAccess(Subject resourceCreatorSubject) {
        return REMOTE_ACCESS.equals(getType()) && REMOTE_ACCESS.equals(resourceCreatorSubject.getType());
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        Subject subject = (Subject) o;
        return version.equals(subject.version)
            && user.equals(subject.user)
            && Objects.equals(realm, subject.realm)
            && type == subject.type
            && metadata.equals(subject.metadata);
    }

    @Override
    public int hashCode() {
        return Objects.hash(version, user, realm, type, metadata);
    }

    @Override
    public String toString() {
        return "Subject{"
            + "version="
            + version
            + ", user="
            + user
            + ", realm="
            + realm
            + ", type="
            + type
            + ", metadata="
            + metadata
            + '}';
    }

    private RoleReferenceIntersection buildRoleReferencesForUser(AnonymousUser anonymousUser) {
        if (user.equals(anonymousUser)) {
            return new RoleReferenceIntersection(new RoleReference.NamedRoleReference(user.roles()));
        }
        final String[] allRoleNames;
        if (anonymousUser == null || false == anonymousUser.enabled()) {
            allRoleNames = user.roles();
        } else {
            // TODO: should we validate enable status and length of role names on instantiation time of anonymousUser?
            if (anonymousUser.roles().length == 0) {
                throw new IllegalStateException("anonymous is only enabled when the anonymous user has roles");
            }
            allRoleNames = ArrayUtils.concat(user.roles(), anonymousUser.roles());
        }
        return new RoleReferenceIntersection(new RoleReference.NamedRoleReference(allRoleNames));
    }

    private RoleReferenceIntersection buildRoleReferencesForApiKey() {
        if (version.before(VERSION_API_KEY_ROLES_AS_BYTES)) {
            return buildRolesReferenceForApiKeyBwc();
        }
        final String apiKeyId = (String) metadata.get(AuthenticationField.API_KEY_ID_KEY);
        final BytesReference roleDescriptorsBytes = (BytesReference) metadata.get(API_KEY_ROLE_DESCRIPTORS_KEY);
        final BytesReference limitedByRoleDescriptorsBytes = getLimitedByRoleDescriptorsBytes();
        if (roleDescriptorsBytes == null && limitedByRoleDescriptorsBytes == null) {
            throw new ElasticsearchSecurityException("no role descriptors found for API key");
        }
        final RoleReference.ApiKeyRoleReference limitedByRoleReference = new RoleReference.ApiKeyRoleReference(
            apiKeyId,
            limitedByRoleDescriptorsBytes,
            RoleReference.ApiKeyRoleType.LIMITED_BY
        );
        if (isEmptyRoleDescriptorsBytes(roleDescriptorsBytes)) {
            return new RoleReferenceIntersection(limitedByRoleReference);
        }
        return new RoleReferenceIntersection(
            new RoleReference.ApiKeyRoleReference(apiKeyId, roleDescriptorsBytes, RoleReference.ApiKeyRoleType.ASSIGNED),
            limitedByRoleReference
        );
    }

    private RoleReferenceIntersection buildRoleReferencesForRemoteAccess() {
        final List<RoleReference> roleReferences = new ArrayList<>(4);
        @SuppressWarnings("unchecked")
        final var remoteAccessRoleDescriptorsBytes = (List<RoleDescriptorsBytes>) metadata.get(
            AuthenticationField.REMOTE_ACCESS_ROLE_DESCRIPTORS_KEY
        );
        if (remoteAccessRoleDescriptorsBytes.isEmpty()) {
            // If the remote access role descriptors are empty, the remote user has no privileges. We need to add an empty role to restrict
            // access of the overall intersection accordingly
            roleReferences.add(new RoleReference.RemoteAccessRoleReference(RoleDescriptorsBytes.EMPTY));
        } else {
            // TODO handle this once we support API keys as querying subjects
            assert remoteAccessRoleDescriptorsBytes.size() == 1
                : "only a singleton list of remote access role descriptors bytes is supported";
            for (RoleDescriptorsBytes roleDescriptorsBytes : remoteAccessRoleDescriptorsBytes) {
                roleReferences.add(new RoleReference.RemoteAccessRoleReference(roleDescriptorsBytes));
            }
        }
        roleReferences.addAll(buildRoleReferencesForApiKey().getRoleReferences());
        return new RoleReferenceIntersection(List.copyOf(roleReferences));
    }

    private static boolean isEmptyRoleDescriptorsBytes(BytesReference roleDescriptorsBytes) {
        return roleDescriptorsBytes == null || (roleDescriptorsBytes.length() == 2 && "{}".equals(roleDescriptorsBytes.utf8ToString()));
    }

    private RoleReferenceIntersection buildRolesReferenceForApiKeyBwc() {
        final String apiKeyId = (String) metadata.get(AuthenticationField.API_KEY_ID_KEY);
        final Map<String, Object> roleDescriptorsMap = getRoleDescriptorMap(API_KEY_ROLE_DESCRIPTORS_KEY);
        final Map<String, Object> limitedByRoleDescriptorsMap = getRoleDescriptorMap(API_KEY_LIMITED_ROLE_DESCRIPTORS_KEY);
        if (roleDescriptorsMap == null && limitedByRoleDescriptorsMap == null) {
            throw new ElasticsearchSecurityException("no role descriptors found for API key");
        } else {
            final RoleReference.BwcApiKeyRoleReference limitedByRoleReference = new RoleReference.BwcApiKeyRoleReference(
                apiKeyId,
                limitedByRoleDescriptorsMap,
                RoleReference.ApiKeyRoleType.LIMITED_BY
            );
            if (roleDescriptorsMap == null || roleDescriptorsMap.isEmpty()) {
                return new RoleReferenceIntersection(limitedByRoleReference);
            } else {
                return new RoleReferenceIntersection(
                    new RoleReference.BwcApiKeyRoleReference(apiKeyId, roleDescriptorsMap, RoleReference.ApiKeyRoleType.ASSIGNED),
                    limitedByRoleReference
                );
            }
        }
    }

    @SuppressWarnings("unchecked")
    private Map<String, Object> getRoleDescriptorMap(String key) {
        return (Map<String, Object>) metadata.get(key);
    }

    // This following fixed role descriptor is for fleet-server BWC on and before 7.14.
    // It is fixed and must NOT be updated when the fleet-server service account updates.
    // Package private for testing
    static final BytesArray FLEET_SERVER_ROLE_DESCRIPTOR_BYTES_V_7_14 = new BytesArray(
        "{\"elastic/fleet-server\":{\"cluster\":[\"monitor\",\"manage_own_api_key\"],"
            + "\"indices\":[{\"names\":[\"logs-*\",\"metrics-*\",\"traces-*\",\"synthetics-*\","
            + "\".logs-endpoint.diagnostic.collection-*\"],"
            + "\"privileges\":[\"write\",\"create_index\",\"auto_configure\"],\"allow_restricted_indices\":false},"
            + "{\"names\":[\".fleet-*\"],\"privileges\":[\"read\",\"write\",\"monitor\",\"create_index\",\"auto_configure\"],"
            + "\"allow_restricted_indices\":false}],\"applications\":[],\"run_as\":[],\"metadata\":{},"
            + "\"transient_metadata\":{\"enabled\":true}}}"
    );

    private BytesReference getLimitedByRoleDescriptorsBytes() {
        final BytesReference bytesReference = (BytesReference) metadata.get(API_KEY_LIMITED_ROLE_DESCRIPTORS_KEY);
        // Unfortunate BWC bug fix code
        if (bytesReference.length() == 2 && "{}".equals(bytesReference.utf8ToString())) {
            if (ServiceAccountSettings.REALM_NAME.equals(metadata.get(AuthenticationField.API_KEY_CREATOR_REALM_NAME))
                && "elastic/fleet-server".equals(user.principal())) {
                return FLEET_SERVER_ROLE_DESCRIPTOR_BYTES_V_7_14;
            }
        }
        return bytesReference;
    }
}
