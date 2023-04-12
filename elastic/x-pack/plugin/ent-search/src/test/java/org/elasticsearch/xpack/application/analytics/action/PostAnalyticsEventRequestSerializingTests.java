/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.analytics.action;

import org.elasticsearch.common.ValidationException;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.test.AbstractWireSerializingTestCase;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xpack.application.analytics.event.AnalyticsEvent;

import java.io.IOException;
import java.util.Collections;
import java.util.List;
import java.util.Locale;

import static org.mockito.Mockito.mock;

public class PostAnalyticsEventRequestSerializingTests extends AbstractWireSerializingTestCase<PostAnalyticsEventAction.Request> {

    public void testValidate() {
        assertNull(createTestInstance().validate());
    }

    public void testValidateInvalidEventTypes() {
        List<String> invalidEventTypes = List.of(randomIdentifier(), randomEventType().toUpperCase(Locale.ROOT));

        for (String eventType : invalidEventTypes) {
            PostAnalyticsEventAction.Request request = new PostAnalyticsEventAction.Request(
                randomIdentifier(),
                eventType,
                randomBoolean(),
                randomLong(),
                randomFrom(XContentType.values()),
                mock(BytesReference.class)
            );

            ValidationException e = request.validate();
            assertNotNull(e);
            assertEquals(Collections.singletonList("invalid event type: [" + eventType + "]"), e.validationErrors());
        }
    }

    @Override
    protected Writeable.Reader<PostAnalyticsEventAction.Request> instanceReader() {
        return PostAnalyticsEventAction.Request::new;
    }

    @Override
    protected PostAnalyticsEventAction.Request createTestInstance() {
        return new PostAnalyticsEventAction.Request(
            randomIdentifier(),
            randomEventType(),
            randomBoolean(),
            randomLong(),
            randomFrom(XContentType.values()),
            new BytesArray(randomByteArrayOfLength(20))
        );
    }

    @Override
    protected PostAnalyticsEventAction.Request mutateInstance(PostAnalyticsEventAction.Request instance) throws IOException {
        return randomValueOtherThan(instance, this::createTestInstance);
    }

    private String randomEventType() {
        return randomFrom(AnalyticsEvent.Type.values()).toString().toLowerCase(Locale.ROOT);
    }
}
