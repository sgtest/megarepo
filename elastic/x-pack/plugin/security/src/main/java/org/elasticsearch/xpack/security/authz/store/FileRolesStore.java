/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.authz.store;

import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.apache.logging.log4j.util.Supplier;
import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.component.AbstractComponent;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.LoggingDeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.yaml.YamlXContent;
import org.elasticsearch.env.Environment;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.watcher.FileChangesListener;
import org.elasticsearch.watcher.FileWatcher;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xpack.core.XPackPlugin;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.security.authz.RoleDescriptor;
import org.elasticsearch.xpack.core.security.authz.RoleDescriptor.IndicesPrivileges;
import org.elasticsearch.xpack.core.security.authz.store.ReservedRolesStore;
import org.elasticsearch.xpack.core.security.support.NoOpLogger;
import org.elasticsearch.xpack.core.security.support.Validation;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.regex.Pattern;

import static java.util.Collections.emptyMap;
import static java.util.Collections.unmodifiableMap;

public class FileRolesStore extends AbstractComponent {

    private static final Pattern IN_SEGMENT_LINE = Pattern.compile("^\\s+.+");
    private static final Pattern SKIP_LINE = Pattern.compile("(^#.*|^\\s*)");

    private final Path file;
    private final XPackLicenseState licenseState;
    private final List<Runnable> listeners = new ArrayList<>();

    private volatile Map<String, RoleDescriptor> permissions;

    public FileRolesStore(Settings settings, Environment env, ResourceWatcherService watcherService, XPackLicenseState licenseState)
            throws IOException {
        this(settings, env, watcherService, () -> {}, licenseState);
    }

    FileRolesStore(Settings settings, Environment env, ResourceWatcherService watcherService, Runnable listener,
                   XPackLicenseState licenseState) throws IOException {
        super(settings);
        this.file = resolveFile(env);
        if (listener != null) {
            listeners.add(listener);
        }
        this.licenseState = licenseState;
        FileWatcher watcher = new FileWatcher(file.getParent());
        watcher.addListener(new FileListener());
        watcherService.add(watcher, ResourceWatcherService.Frequency.HIGH);
        permissions = parseFile(file, logger, settings, licenseState);
    }

    public Set<RoleDescriptor> roleDescriptors(Set<String> roleNames) {
        Set<RoleDescriptor> descriptors = new HashSet<>();
        roleNames.forEach((name) -> {
            RoleDescriptor descriptor = permissions.get(name);
            if (descriptor != null) {
                descriptors.add(descriptor);
            }
        });
        return descriptors;
    }

    public Map<String, Object> usageStats() {
        Map<String, Object> usageStats = new HashMap<>();
        usageStats.put("size", permissions.size());

        boolean dls = false;
        boolean fls = false;
        for (RoleDescriptor descriptor : permissions.values()) {
            for (IndicesPrivileges indicesPrivileges : descriptor.getIndicesPrivileges()) {
                fls = fls || indicesPrivileges.getGrantedFields() != null || indicesPrivileges.getDeniedFields() != null;
                dls = dls || indicesPrivileges.getQuery() != null;
            }
            if (fls && dls) {
                break;
            }
        }
        usageStats.put("fls", fls);
        usageStats.put("dls", dls);

        return usageStats;
    }

    public void addListener(Runnable runnable) {
        Objects.requireNonNull(runnable);
        synchronized (this) {
            listeners.add(runnable);
        }
    }

    public Path getFile() {
        return file;
    }

    public static Path resolveFile(Environment env) {
        return XPackPlugin.resolveConfigFile(env, "roles.yml");
    }

    public static Set<String> parseFileForRoleNames(Path path, Logger logger) {
        return parseRoleDescriptors(path, logger, false, Settings.EMPTY).keySet();
    }

    public static Map<String, RoleDescriptor> parseFile(Path path, Logger logger, Settings settings, XPackLicenseState licenseState) {
        return parseFile(path, logger, true, settings, licenseState);
    }

    public static Map<String, RoleDescriptor> parseFile(Path path, Logger logger, boolean resolvePermission,
                                                                     Settings settings, XPackLicenseState licenseState) {
        if (logger == null) {
            logger = NoOpLogger.INSTANCE;
        }

        Map<String, RoleDescriptor> roles = new HashMap<>();
        logger.debug("attempting to read roles file located at [{}]", path.toAbsolutePath());
        if (Files.exists(path)) {
            try {
                List<String> roleSegments = roleSegments(path);
                final boolean flsDlsLicensed = licenseState.isDocumentAndFieldLevelSecurityAllowed();
                for (String segment : roleSegments) {
                    RoleDescriptor descriptor = parseRoleDescriptor(segment, path, logger, resolvePermission, settings);
                    if (descriptor != null) {
                        if (ReservedRolesStore.isReserved(descriptor.getName())) {
                            logger.warn("role [{}] is reserved. the relevant role definition in the mapping file will be ignored",
                                    descriptor.getName());
                        } else if (flsDlsLicensed == false && descriptor.isUsingDocumentOrFieldLevelSecurity()) {
                            logger.warn("role [{}] uses document and/or field level security, which is not enabled by the current license" +
                                    ". this role will be ignored", descriptor.getName());
                            // we still put the role in the map to avoid unnecessary negative lookups
                            roles.put(descriptor.getName(), descriptor);
                        } else {
                            roles.put(descriptor.getName(), descriptor);
                        }
                    }
                }
            } catch (IOException ioe) {
                logger.error(
                        (Supplier<?>) () -> new ParameterizedMessage(
                                "failed to read roles file [{}]. skipping all roles...",
                                path.toAbsolutePath()),
                        ioe);
                return emptyMap();
            }
        } else {
            logger.debug("roles file does not exist");
            return emptyMap();
        }

        logger.info("parsed [{}] roles from file [{}]", roles.size(), path.toAbsolutePath());
        return unmodifiableMap(roles);
    }

    public static Map<String, RoleDescriptor> parseRoleDescriptors(Path path, Logger logger, boolean resolvePermission, Settings settings) {
        if (logger == null) {
            logger = NoOpLogger.INSTANCE;
        }

        Map<String, RoleDescriptor> roles = new HashMap<>();
        logger.trace("attempting to read roles file located at [{}]", path.toAbsolutePath());
        if (Files.exists(path)) {
            try {
                List<String> roleSegments = roleSegments(path);
                for (String segment : roleSegments) {
                    RoleDescriptor rd = parseRoleDescriptor(segment, path, logger, resolvePermission, settings);
                    if (rd != null) {
                        roles.put(rd.getName(), rd);
                    }
                }
            } catch (IOException ioe) {
                logger.error(
                        (Supplier<?>) () -> new ParameterizedMessage(
                                "failed to read roles file [{}]. skipping all roles...",
                                path.toAbsolutePath()),
                        ioe);
                return emptyMap();
            }
        }
        return unmodifiableMap(roles);
    }

    @Nullable
    static RoleDescriptor parseRoleDescriptor(String segment, Path path, Logger logger, boolean resolvePermissions, Settings settings) {
        String roleName = null;
        try {
            // EMPTY is safe here because we never use namedObject
            XContentParser parser = YamlXContent.yamlXContent
                    .createParser(NamedXContentRegistry.EMPTY, LoggingDeprecationHandler.INSTANCE, segment);
            XContentParser.Token token = parser.nextToken();
            if (token == XContentParser.Token.START_OBJECT) {
                token = parser.nextToken();
                if (token == XContentParser.Token.FIELD_NAME) {
                    roleName = parser.currentName();
                    Validation.Error validationError = Validation.Roles.validateRoleName(roleName);
                    if (validationError != null) {
                        logger.error("invalid role definition [{}] in roles file [{}]. invalid role name - {}. skipping role... ",
                                roleName, path.toAbsolutePath(), validationError);
                        return null;
                    }

                    if (resolvePermissions == false) {
                        return new RoleDescriptor(roleName, null, null, null);
                    }

                    token = parser.nextToken();
                    if (token == XContentParser.Token.START_OBJECT) {
                        // we pass true as last parameter because we do not want to reject files if field permissions
                        // are given in 2.x syntax
                        RoleDescriptor descriptor = RoleDescriptor.parse(roleName, parser, true);
                        return checkDescriptor(descriptor, path, logger, settings);
                    } else {
                        logger.error("invalid role definition [{}] in roles file [{}]. skipping role...", roleName, path.toAbsolutePath());
                        return null;
                    }
                }
            }
            logger.error("invalid role definition [{}] in roles file [{}]. skipping role...", roleName, path.toAbsolutePath());
        } catch (ElasticsearchParseException e) {
            assert roleName != null;
            if (logger.isDebugEnabled()) {
                final String finalRoleName = roleName;
                logger.debug((Supplier<?>) () -> new ParameterizedMessage("parsing exception for role [{}]", finalRoleName), e);
            } else {
                logger.error(e.getMessage() + ". skipping role...");
            }
        } catch (IOException e) {
            if (roleName != null) {
                final String finalRoleName = roleName;
                logger.error(
                        (Supplier<?>) () -> new ParameterizedMessage(
                                "invalid role definition [{}] in roles file [{}]. skipping role...",
                                finalRoleName,
                                path),
                        e);
            } else {
                logger.error(
                        (Supplier<?>) () -> new ParameterizedMessage(
                                "invalid role definition in roles file [{}]. skipping role...",
                                path),
                        e);
            }
        }
        return null;
    }

    @Nullable
    private static RoleDescriptor checkDescriptor(RoleDescriptor descriptor, Path path, Logger logger, Settings settings) {
        String roleName = descriptor.getName();
        // first check if FLS/DLS is enabled on the role...
        for (RoleDescriptor.IndicesPrivileges privilege : descriptor.getIndicesPrivileges()) {
            if ((privilege.getQuery() != null || privilege.getGrantedFields() != null || privilege.getDeniedFields() != null)
                    && XPackSettings.DLS_FLS_ENABLED.get(settings) == false) {
                logger.error("invalid role definition [{}] in roles file [{}]. document and field level security is not " +
                        "enabled. set [{}] to [true] in the configuration file. skipping role...", roleName, path
                        .toAbsolutePath(), XPackSettings.DLS_FLS_ENABLED.getKey());
                return null;
            }
        }
        return descriptor;
    }

    private static List<String> roleSegments(Path path) throws IOException {
        List<String> segments = new ArrayList<>();
        StringBuilder builder = null;
        for (String line : Files.readAllLines(path, StandardCharsets.UTF_8)) {
            if (!SKIP_LINE.matcher(line).matches()) {
                if (IN_SEGMENT_LINE.matcher(line).matches()) {
                    if (builder != null) {
                        builder.append(line).append("\n");
                    }
                } else {
                    if (builder != null) {
                        segments.add(builder.toString());
                    }
                    builder = new StringBuilder(line).append("\n");
                }
            }
        }
        if (builder != null) {
            segments.add(builder.toString());
        }
        return segments;
    }

    private class FileListener implements FileChangesListener {

        @Override
        public void onFileCreated(Path file) {
            onFileChanged(file);
        }

        @Override
        public void onFileDeleted(Path file) {
            onFileChanged(file);
        }

        @Override
        public void onFileChanged(Path file) {
            if (file.equals(FileRolesStore.this.file)) {
                try {
                    permissions = parseFile(file, logger, settings, licenseState);
                    logger.info("updated roles (roles file [{}] {})", file.toAbsolutePath(), Files.exists(file) ? "changed" : "removed");
                } catch (Exception e) {
                    logger.error(
                            (Supplier<?>) () -> new ParameterizedMessage(
                                    "could not reload roles file [{}]. Current roles remain unmodified", file.toAbsolutePath()), e);
                    return;
                }

                synchronized (FileRolesStore.this) {
                    listeners.forEach(Runnable::run);
                }
            }
        }
    }
}
