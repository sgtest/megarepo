/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.telemetry;

import org.elasticsearch.core.Strings;
import org.elasticsearch.telemetry.metric.Instrument;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * Container for registered Instruments (either {@link Instrument} or Otel's versions).
 * Records invocations of the Instruments as {@link Measurement}s.
 * @param <I> The supertype of the registered instrument.
 */
class MetricRecorder<I> {

    /**
     * Container for Instrument of a given type, such as DoubleGauge, LongHistogram, etc.
     * @param registered - registration records for each named metrics
     * @param called - one instance per invocation of the instance
     * @param instruments - the instrument instance
     */
    private record RegisteredMetric<I>(
        Map<String, Registration> registered,
        Map<String, List<Measurement>> called,
        Map<String, I> instruments
    ) {
        void register(String name, String description, String unit, I instrument) {
            assert registered.containsKey(name) == false
                : Strings.format("unexpected [{}]: [{}][{}], already registered[{}]", name, description, unit, registered.get(name));
            registered.put(name, new Registration(name, description, unit));
            instruments.put(name, instrument);
        }

        void call(String name, Measurement call) {
            assert registered.containsKey(name) : Strings.format("call for unregistered metric [{}]: [{}]", name, call);
            called.computeIfAbsent(Objects.requireNonNull(name), k -> new ArrayList<>()).add(call);
        }
    }

    /**
     * The containers for each metric type.
     */
    private final Map<InstrumentType, RegisteredMetric<I>> metrics;

    MetricRecorder() {
        metrics = new HashMap<>(InstrumentType.values().length);
        for (var instrument : InstrumentType.values()) {
            metrics.put(instrument, new RegisteredMetric<>(new HashMap<>(), new HashMap<>(), new HashMap<>()));
        }
    }

    /**
     * Register an instrument.  Instruments must be registered before they are used.
     */
    void register(I instrument, InstrumentType instrumentType, String name, String description, String unit) {
        metrics.get(instrumentType).register(name, description, unit, instrument);
    }

    /**
     * Record a call made to a registered Elasticsearch {@link Instrument}.
     */
    void call(Instrument instrument, Number value, Map<String, Object> attributes) {
        call(InstrumentType.fromInstrument(instrument), instrument.getName(), value, attributes);
    }

    /**
     * Record a call made to the registered instrument represented by the {@link InstrumentType} enum.
     */
    void call(InstrumentType instrumentType, String name, Number value, Map<String, Object> attributes) {
        metrics.get(instrumentType).call(name, new Measurement(value, attributes, instrumentType.isDouble));
    }

    /**
     * Get the {@link Measurement}s for each call of the given registered Elasticsearch {@link Instrument}.
     */
    public List<Measurement> getMeasurements(Instrument instrument) {
        return getMeasurements(InstrumentType.fromInstrument(instrument), instrument.getName());
    }

    List<Measurement> getMeasurements(InstrumentType instrumentType, String name) {
        return metrics.get(instrumentType).called.getOrDefault(Objects.requireNonNull(name), Collections.emptyList());
    }

    /**
     * Get the {@link Registration} for a given elasticsearch {@link Instrument}.
     */
    Registration getRegistration(Instrument instrument) {
        return metrics.get(InstrumentType.fromInstrument(instrument)).registered().get(instrument.getName());
    }

    /**
     * Fetch the instrument instance given the type and registered name.
     */
    I getInstrument(InstrumentType instrumentType, String name) {
        return metrics.get(instrumentType).instruments.get(name);
    }
}
