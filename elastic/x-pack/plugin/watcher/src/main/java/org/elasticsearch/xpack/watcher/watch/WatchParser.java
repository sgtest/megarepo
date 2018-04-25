/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.watcher.watch;

import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.component.AbstractComponent;
import org.elasticsearch.common.lucene.uid.Versions;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.LoggingDeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.xpack.core.watcher.actions.ActionRegistry;
import org.elasticsearch.xpack.core.watcher.actions.ActionStatus;
import org.elasticsearch.xpack.core.watcher.actions.ActionWrapper;
import org.elasticsearch.xpack.core.watcher.common.secret.Secret;
import org.elasticsearch.xpack.core.watcher.condition.ExecutableCondition;
import org.elasticsearch.xpack.core.watcher.crypto.CryptoService;
import org.elasticsearch.xpack.core.watcher.input.ExecutableInput;
import org.elasticsearch.xpack.core.watcher.support.WatcherDateTimeUtils;
import org.elasticsearch.xpack.core.watcher.support.xcontent.WatcherXContentParser;
import org.elasticsearch.xpack.core.watcher.transform.ExecutableTransform;
import org.elasticsearch.xpack.core.watcher.trigger.Trigger;
import org.elasticsearch.xpack.core.watcher.watch.Watch;
import org.elasticsearch.xpack.core.watcher.watch.WatchField;
import org.elasticsearch.xpack.core.watcher.watch.WatchStatus;
import org.elasticsearch.xpack.watcher.condition.InternalAlwaysCondition;
import org.elasticsearch.xpack.watcher.input.InputRegistry;
import org.elasticsearch.xpack.watcher.input.none.ExecutableNoneInput;
import org.elasticsearch.xpack.watcher.trigger.TriggerService;
import org.joda.time.DateTime;

import java.io.IOException;
import java.io.InputStream;
import java.time.Clock;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static java.util.Collections.unmodifiableMap;
import static org.elasticsearch.common.unit.TimeValue.timeValueMillis;
import static org.elasticsearch.xpack.core.watcher.support.Exceptions.ioException;
import static org.joda.time.DateTimeZone.UTC;

public class WatchParser extends AbstractComponent {

    private final TriggerService triggerService;
    private final ActionRegistry actionRegistry;
    private final InputRegistry inputRegistry;
    private final CryptoService cryptoService;
    private final Clock clock;
    private final ExecutableInput defaultInput;
    private final ExecutableCondition defaultCondition;
    private final List<ActionWrapper> defaultActions;

    public WatchParser(Settings settings, TriggerService triggerService, ActionRegistry actionRegistry, InputRegistry inputRegistry,
                       @Nullable CryptoService cryptoService, Clock clock) {
        super(settings);
        this.triggerService = triggerService;
        this.actionRegistry = actionRegistry;
        this.inputRegistry = inputRegistry;
        this.cryptoService = cryptoService;
        this.clock = clock;
        this.defaultInput = new ExecutableNoneInput(logger);
        this.defaultCondition = InternalAlwaysCondition.INSTANCE;
        this.defaultActions = Collections.emptyList();
    }

    public Watch parse(String name, boolean includeStatus, BytesReference source, XContentType xContentType) throws IOException {
        return parse(name, includeStatus, false, source, new DateTime(clock.millis(), UTC), xContentType, false);
    }

    public Watch parse(String name, boolean includeStatus, BytesReference source, DateTime now,
                       XContentType xContentType) throws IOException {
        return parse(name, includeStatus, false, source, now, xContentType, false);
    }

    /**
     * Parses the watch represented by the given source. When parsing, any sensitive data that the
     * source might contain (e.g. passwords) will be converted to {@link Secret secrets}
     * Such that the returned watch will potentially hide this sensitive data behind a "secret". A secret
     * is an abstraction around sensitive data (text). When security is enabled, the
     * {@link CryptoService} is used to encrypt the secrets.
     *
     * This method is only called once - when the user adds a new watch. From that moment on, all representations
     * of the watch in the system will be use secrets for sensitive data.
     *
     */
    public Watch parseWithSecrets(String id, boolean includeStatus, BytesReference source, DateTime now,
                                                  XContentType xContentType, boolean allowRedactedPasswords) throws IOException {
        return parse(id, includeStatus, true, source, now, xContentType, allowRedactedPasswords);
    }

    public Watch parseWithSecrets(String id, boolean includeStatus, BytesReference source, DateTime now,
                                  XContentType xContentType) throws IOException {
        return parse(id, includeStatus, true, source, now, xContentType, false);
    }

    private Watch parse(String id, boolean includeStatus, boolean withSecrets, BytesReference source, DateTime now,
                        XContentType xContentType, boolean allowRedactedPasswords) throws IOException {
        if (logger.isTraceEnabled()) {
            logger.trace("parsing watch [{}] ", source.utf8ToString());
        }
        // EMPTY is safe here because we never use namedObject
        try (InputStream stream = source.streamInput();
             WatcherXContentParser parser = new WatcherXContentParser(xContentType.xContent().createParser(NamedXContentRegistry.EMPTY,
                     LoggingDeprecationHandler.INSTANCE, stream),
                     now, withSecrets ? cryptoService : null, allowRedactedPasswords)) {
            parser.nextToken();
            return parse(id, includeStatus, parser);
        } catch (IOException ioe) {
            throw ioException("could not parse watch [{}]", ioe, id);
        }
    }

    public Watch parse(String id, boolean includeStatus, WatcherXContentParser parser) throws IOException {
        Trigger trigger = null;
        ExecutableInput input = defaultInput;
        ExecutableCondition condition = defaultCondition;
        List<ActionWrapper> actions = defaultActions;
        ExecutableTransform transform = null;
        TimeValue throttlePeriod = null;
        Map<String, Object> metatdata = null;
        WatchStatus status = null;
        long version = Versions.MATCH_ANY;

        String currentFieldName = null;
        XContentParser.Token token;
        while ((token = parser.nextToken()) != XContentParser.Token.END_OBJECT) {
            if (token == null ) {
                throw new ElasticsearchParseException("could not parse watch [{}]. null token", id);
            } else if (token == XContentParser.Token.FIELD_NAME) {
                currentFieldName = parser.currentName();
            } else if (token == null || currentFieldName == null) {
                throw new ElasticsearchParseException("could not parse watch [{}], unexpected token [{}]", id, token);
            } else if (WatchField.TRIGGER.match(currentFieldName, parser.getDeprecationHandler())) {
                trigger = triggerService.parseTrigger(id, parser);
            } else if (WatchField.INPUT.match(currentFieldName, parser.getDeprecationHandler())) {
                input = inputRegistry.parse(id, parser);
            } else if (WatchField.CONDITION.match(currentFieldName, parser.getDeprecationHandler())) {
                condition = actionRegistry.getConditionRegistry().parseExecutable(id, parser);
            } else if (WatchField.TRANSFORM.match(currentFieldName, parser.getDeprecationHandler())) {
                transform = actionRegistry.getTransformRegistry().parse(id, parser);
            } else if (WatchField.THROTTLE_PERIOD.match(currentFieldName, parser.getDeprecationHandler())) {
                throttlePeriod = timeValueMillis(parser.longValue());
            } else if (WatchField.THROTTLE_PERIOD_HUMAN.match(currentFieldName, parser.getDeprecationHandler())) {
                // Parser for human specified and 2.x backwards compatible throttle period
                try {
                    throttlePeriod = WatcherDateTimeUtils.parseTimeValue(parser, WatchField.THROTTLE_PERIOD_HUMAN.toString());
                } catch (ElasticsearchParseException pe) {
                    throw new ElasticsearchParseException("could not parse watch [{}]. failed to parse time value for field [{}]",
                            pe, id, currentFieldName);
                }
            } else if (WatchField.ACTIONS.match(currentFieldName, parser.getDeprecationHandler())) {
                actions = actionRegistry.parseActions(id, parser);
            } else if (WatchField.METADATA.match(currentFieldName, parser.getDeprecationHandler())) {
                metatdata = parser.map();
            } else if (WatchField.VERSION.match(currentFieldName, parser.getDeprecationHandler())) {
                version = parser.longValue();
            } else if (WatchField.STATUS.match(currentFieldName, parser.getDeprecationHandler())) {
                if (includeStatus) {
                    status = WatchStatus.parse(id, parser);
                } else {
                    parser.skipChildren();
                }
            } else {
                throw new ElasticsearchParseException("could not parse watch [{}]. unexpected field [{}]", id, currentFieldName);
            }
        }
        if (trigger == null) {
            throw new ElasticsearchParseException("could not parse watch [{}]. missing required field [{}]", id,
                    WatchField.TRIGGER.getPreferredName());
        }

        if (status != null) {
            // verify the status is valid (that every action indeed has a status)
            for (ActionWrapper action : actions) {
                if (status.actionStatus(action.id()) == null) {
                    throw new ElasticsearchParseException("could not parse watch [{}]. watch status in invalid state. action [{}] " +
                            "status is missing", id, action.id());
                }
            }
        } else {
            // we need to create the initial statuses for the actions
            Map<String, ActionStatus> actionsStatuses = new HashMap<>();
            for (ActionWrapper action : actions) {
                actionsStatuses.put(action.id(), new ActionStatus(parser.getParseDateTime()));
            }
            status = new WatchStatus(parser.getParseDateTime(), unmodifiableMap(actionsStatuses));
        }


        return new Watch(id, trigger, input, condition, transform, throttlePeriod, actions,  metatdata, status, version);
    }
}
