/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.telemetry.apm.internal.metrics;

import io.opentelemetry.api.OpenTelemetry;
import io.opentelemetry.api.metrics.DoubleCounterBuilder;
import io.opentelemetry.api.metrics.DoubleGaugeBuilder;
import io.opentelemetry.api.metrics.DoubleHistogramBuilder;
import io.opentelemetry.api.metrics.LongCounter;
import io.opentelemetry.api.metrics.LongCounterBuilder;
import io.opentelemetry.api.metrics.LongUpDownCounterBuilder;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.api.metrics.ObservableLongCounter;
import io.opentelemetry.api.metrics.ObservableLongMeasurement;

import org.elasticsearch.test.ESTestCase;

import java.util.concurrent.CountDownLatch;
import java.util.function.Consumer;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.sameInstance;

public class InstrumentsConcurrencyTests extends ESTestCase {
    private final String name = "name";
    private final String description = "desc";
    private final String unit = "kg";
    private final Meter noopMeter = OpenTelemetry.noop().getMeter("noop");
    private final CountDownLatch buildLatch = new CountDownLatch(1);
    private final CountDownLatch registerLatch = new CountDownLatch(1);
    private final Meter lockingMeter = new Meter() {
        @Override
        public LongCounterBuilder counterBuilder(String name) {
            return new LockingLongCounterBuilder();
        }

        @Override
        public LongUpDownCounterBuilder upDownCounterBuilder(String name) {
            return null;
        }

        @Override
        public DoubleHistogramBuilder histogramBuilder(String name) {
            return null;
        }

        @Override
        public DoubleGaugeBuilder gaugeBuilder(String name) {
            return null;
        }
    };

    class LockingLongCounterBuilder implements LongCounterBuilder {

        @Override
        public LongCounterBuilder setDescription(String description) {
            return this;
        }

        @Override
        public LongCounterBuilder setUnit(String unit) {
            return this;
        }

        @Override
        public DoubleCounterBuilder ofDoubles() {
            return null;
        }

        @Override
        public LongCounter build() {
            try {
                buildLatch.countDown();
                registerLatch.await();
            } catch (Exception e) {
                throw new RuntimeException(e);
            }
            return null;
        }

        @Override
        public ObservableLongCounter buildWithCallback(Consumer<ObservableLongMeasurement> callback) {
            return null;
        }
    }

    public void testLockingWhenRegistering() throws Exception {
        Instruments instruments = new Instruments(lockingMeter);

        var registerThread = new Thread(() -> instruments.registerLongCounter(name, description, unit));
        // registerThread has a countDown latch that is simulating a long-running registration
        registerThread.start();
        buildLatch.await(); // wait for registerThread to hold the lock

        var setProviderThread = new Thread(() -> instruments.setProvider(noopMeter));
        // a setProviderThread will attempt to override a meter, but will wait to acquireLock
        setProviderThread.start();

        // assert that a thread is waiting for a lock during long-running registration
        assertBusy(() -> assertThat(setProviderThread.getState(), equalTo(Thread.State.WAITING)));
        // assert that the old lockingMeter is still in place
        assertBusy(() -> assertThat(instruments.getMeter(), sameInstance(lockingMeter)));

        // finish long-running registration
        registerLatch.countDown();
        // assert that a meter was overriden
        assertBusy(() -> assertThat(instruments.getMeter(), sameInstance(lockingMeter)));

    }
}
