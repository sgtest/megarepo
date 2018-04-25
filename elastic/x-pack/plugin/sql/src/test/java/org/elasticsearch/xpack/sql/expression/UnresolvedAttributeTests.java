/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
/*
* ELASTICSEARCH CONFIDENTIAL
* __________________
*
*  [2017] Elasticsearch Incorporated. All Rights Reserved.
*
* NOTICE:  All information contained herein is, and remains
* the property of Elasticsearch Incorporated and its suppliers,
* if any.  The intellectual and technical concepts contained
* herein are proprietary to Elasticsearch Incorporated
* and its suppliers and may be covered by U.S. and Foreign Patents,
* patents in process, and are protected by trade secret or copyright law.
* Dissemination of this information or reproduction of this material
* is strictly forbidden unless prior written permission is obtained
* from Elasticsearch Incorporated.
*/

package org.elasticsearch.xpack.sql.expression;

import org.elasticsearch.xpack.sql.tree.AbstractNodeTestCase;
import org.elasticsearch.xpack.sql.tree.Location;

import java.util.Arrays;
import java.util.Objects;
import java.util.function.Supplier;

import static org.elasticsearch.xpack.sql.tree.LocationTests.randomLocation;

public class UnresolvedAttributeTests extends AbstractNodeTestCase<UnresolvedAttribute, Expression> {
    public static UnresolvedAttribute randomUnresolvedAttribute() {
        Location location = randomLocation();
        String name = randomAlphaOfLength(5);
        String qualifier = randomQualifier();
        ExpressionId id = randomBoolean() ? null : new ExpressionId();
        String unresolvedMessage = randomUnresolvedMessage();
        Object resolutionMetadata = new Object();
        return new UnresolvedAttribute(location, name, qualifier, id, unresolvedMessage, resolutionMetadata);
    }

    /**
     * A random qualifier. It is important that this be distinct
     * from the name and the unresolvedMessage for testing transform.
     */
    private static String randomQualifier() {
        return randomBoolean() ? null : randomAlphaOfLength(6);
    }

    /**
     * A random qualifier. It is important that this be distinct
     * from the name and the qualifier for testing transform.
     */
    private static String randomUnresolvedMessage() {
        return randomAlphaOfLength(7);
    }

    @Override
    protected UnresolvedAttribute randomInstance() {
        return randomUnresolvedAttribute();
    }

    @Override
    protected UnresolvedAttribute mutate(UnresolvedAttribute a) {
        Supplier<UnresolvedAttribute> option = randomFrom(Arrays.asList(
            () -> new UnresolvedAttribute(a.location(),
                    randomValueOtherThan(a.name(), () -> randomAlphaOfLength(5)),
                    a.qualifier(), a.id(), a.unresolvedMessage(), a.resolutionMetadata()),
            () -> new UnresolvedAttribute(a.location(), a.name(),
                    randomValueOtherThan(a.qualifier(), UnresolvedAttributeTests::randomQualifier),
                    a.id(), a.unresolvedMessage(), a.resolutionMetadata()),
            () -> new UnresolvedAttribute(a.location(), a.name(), a.qualifier(),
                    new ExpressionId(), a.unresolvedMessage(), a.resolutionMetadata()),
            () -> new UnresolvedAttribute(a.location(), a.name(), a.qualifier(), a.id(),
                    randomValueOtherThan(a.unresolvedMessage(), () -> randomUnresolvedMessage()),
                    a.resolutionMetadata()),
            () -> new UnresolvedAttribute(a.location(), a.name(),
                    a.qualifier(), a.id(), a.unresolvedMessage(), new Object())
        ));
        return option.get();
    }

    @Override
    protected UnresolvedAttribute copy(UnresolvedAttribute a) {
        return new UnresolvedAttribute(a.location(), a.name(), a.qualifier(), a.id(), a.unresolvedMessage(), a.resolutionMetadata());
    }

    @Override
    public void testTransform() {
        UnresolvedAttribute a = randomUnresolvedAttribute();

        String newName = randomValueOtherThan(a.name(), () -> randomAlphaOfLength(5));
        assertEquals(new UnresolvedAttribute(a.location(), newName, a.qualifier(), a.id(),
                a.unresolvedMessage(), a.resolutionMetadata()),
            a.transformPropertiesOnly(v -> Objects.equals(v, a.name()) ? newName : v, Object.class));

        String newQualifier = randomValueOtherThan(a.qualifier(), UnresolvedAttributeTests::randomQualifier);
        assertEquals(new UnresolvedAttribute(a.location(), a.name(), newQualifier, a.id(),
                a.unresolvedMessage(), a.resolutionMetadata()),
            a.transformPropertiesOnly(v -> Objects.equals(v, a.qualifier()) ? newQualifier : v, Object.class));

        ExpressionId newId = new ExpressionId();
        assertEquals(new UnresolvedAttribute(a.location(), a.name(), a.qualifier(), newId,
                a.unresolvedMessage(), a.resolutionMetadata()),
            a.transformPropertiesOnly(v -> Objects.equals(v, a.id()) ? newId : v, Object.class));

        String newMessage = randomValueOtherThan(a.unresolvedMessage(), UnresolvedAttributeTests::randomUnresolvedMessage);
        assertEquals(new UnresolvedAttribute(a.location(), a.name(), a.qualifier(), a.id(),
                newMessage, a.resolutionMetadata()),
            a.transformPropertiesOnly(v -> Objects.equals(v, a.unresolvedMessage()) ? newMessage : v, Object.class));

        Object newMeta = new Object();
        assertEquals(new UnresolvedAttribute(a.location(), a.name(), a.qualifier(), a.id(),
                a.unresolvedMessage(), newMeta),
            a.transformPropertiesOnly(v -> Objects.equals(v, a.resolutionMetadata()) ? newMeta : v, Object.class));
    }

    @Override
    public void testReplaceChildren() {
        // UnresolvedAttribute doesn't have any children
    }
}
