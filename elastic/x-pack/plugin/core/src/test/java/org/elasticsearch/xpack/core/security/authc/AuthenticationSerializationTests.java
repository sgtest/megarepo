/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.security.authc;

import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.security.user.AsyncSearchUser;
import org.elasticsearch.xpack.core.security.user.ElasticUser;
import org.elasticsearch.xpack.core.security.user.KibanaSystemUser;
import org.elasticsearch.xpack.core.security.user.KibanaUser;
import org.elasticsearch.xpack.core.security.user.SystemUser;
import org.elasticsearch.xpack.core.security.user.User;
import org.elasticsearch.xpack.core.security.user.XPackUser;

import java.util.Arrays;

import static org.elasticsearch.xpack.core.security.authc.Authentication.AuthenticationSerializationHelper;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.sameInstance;

public class AuthenticationSerializationTests extends ESTestCase {

    public void testWriteToAndReadFrom() throws Exception {
        User user = new User(randomAlphaOfLengthBetween(4, 30), generateRandomStringArray(20, 30, false));
        BytesStreamOutput output = new BytesStreamOutput();

        AuthenticationSerializationHelper.writeUserTo(user, output);
        User readFrom = AuthenticationSerializationHelper.readUserFrom(output.bytes().streamInput());

        assertThat(readFrom, not(sameInstance(user)));
        assertThat(readFrom.principal(), is(user.principal()));
        assertThat(Arrays.equals(readFrom.roles(), user.roles()), is(true));
        assertThat(readFrom, not(instanceOf(Authentication.RunAsUser.class)));
    }

    public void testWriteToAndReadFromWithRunAs() throws Exception {
        User authUser = new User(randomAlphaOfLengthBetween(4, 30), generateRandomStringArray(20, 30, false));
        User user = new Authentication.RunAsUser(
            new User(randomAlphaOfLengthBetween(4, 30), randomBoolean() ? generateRandomStringArray(20, 30, false) : null),
            authUser
        );

        BytesStreamOutput output = new BytesStreamOutput();

        AuthenticationSerializationHelper.writeUserTo(user, output);
        User readFrom = AuthenticationSerializationHelper.readUserFrom(output.bytes().streamInput());

        assertThat(readFrom, not(sameInstance(user)));
        assertThat(readFrom.principal(), is(user.principal()));
        assertThat(Arrays.equals(readFrom.roles(), user.roles()), is(true));

        assertThat(readFrom, instanceOf(Authentication.RunAsUser.class));
        User readFromAuthUser = ((Authentication.RunAsUser) readFrom).authenticatingUser;
        assertThat(authUser, is(notNullValue()));
        assertThat(readFromAuthUser.principal(), is(authUser.principal()));
        assertThat(Arrays.equals(readFromAuthUser.roles(), authUser.roles()), is(true));
        assertThat(readFromAuthUser, not(instanceOf(Authentication.RunAsUser.class)));
    }

    public void testSystemUserReadAndWrite() throws Exception {
        BytesStreamOutput output = new BytesStreamOutput();

        AuthenticationSerializationHelper.writeUserTo(SystemUser.INSTANCE, output);
        User readFrom = AuthenticationSerializationHelper.readUserFrom(output.bytes().streamInput());

        assertThat(readFrom, is(sameInstance(SystemUser.INSTANCE)));
        assertThat(readFrom, not(instanceOf(Authentication.RunAsUser.class)));
    }

    public void testXPackUserReadAndWrite() throws Exception {
        BytesStreamOutput output = new BytesStreamOutput();

        AuthenticationSerializationHelper.writeUserTo(XPackUser.INSTANCE, output);
        User readFrom = AuthenticationSerializationHelper.readUserFrom(output.bytes().streamInput());

        assertThat(readFrom, is(sameInstance(XPackUser.INSTANCE)));
        assertThat(readFrom, not(instanceOf(Authentication.RunAsUser.class)));
    }

    public void testAsyncSearchUserReadAndWrite() throws Exception {
        BytesStreamOutput output = new BytesStreamOutput();

        AuthenticationSerializationHelper.writeUserTo(AsyncSearchUser.INSTANCE, output);
        User readFrom = AuthenticationSerializationHelper.readUserFrom(output.bytes().streamInput());

        assertThat(readFrom, is(sameInstance(AsyncSearchUser.INSTANCE)));
        assertThat(readFrom, not(instanceOf(Authentication.RunAsUser.class)));
    }

    public void testFakeInternalUserSerialization() throws Exception {
        BytesStreamOutput output = new BytesStreamOutput();
        output.writeBoolean(true);
        output.writeString(randomAlphaOfLengthBetween(4, 30));
        try {
            AuthenticationSerializationHelper.readUserFrom(output.bytes().streamInput());
            fail("system user had wrong name");
        } catch (IllegalStateException e) {
            // expected
        }
    }

    public void testReservedUserSerialization() throws Exception {
        BytesStreamOutput output = new BytesStreamOutput();
        final ElasticUser elasticUser = new ElasticUser(true);
        AuthenticationSerializationHelper.writeUserTo(elasticUser, output);
        User readFrom = AuthenticationSerializationHelper.readUserFrom(output.bytes().streamInput());

        assertEquals(elasticUser, readFrom);

        final KibanaUser kibanaUser = new KibanaUser(true);
        output = new BytesStreamOutput();
        AuthenticationSerializationHelper.writeUserTo(kibanaUser, output);
        readFrom = AuthenticationSerializationHelper.readUserFrom(output.bytes().streamInput());

        assertEquals(kibanaUser, readFrom);

        final KibanaSystemUser kibanaSystemUser = new KibanaSystemUser(true);
        output = new BytesStreamOutput();
        AuthenticationSerializationHelper.writeUserTo(kibanaSystemUser, output);
        readFrom = AuthenticationSerializationHelper.readUserFrom(output.bytes().streamInput());

        assertEquals(kibanaSystemUser, readFrom);
    }
}
