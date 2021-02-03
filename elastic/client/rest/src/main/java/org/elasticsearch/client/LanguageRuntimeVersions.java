/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.client;

import java.lang.reflect.Field;
import java.lang.reflect.Method;

// Copied verbatim from https://github.com/elastic/jvm-languages-sniffer

class LanguageRuntimeVersions {

    /**
     * Returns runtime information by looking up classes identifying non-Java JVM
     * languages and appending a key with their name and their major.minor version, if available
     */
    public static String getRuntimeMetadata() {
        StringBuilder s = new StringBuilder();
        String version;

        version= kotlinVersion();
        if (version != null) {
            s.append(",kt=").append(version);
        }

        version = scalaVersion();
        if (version != null) {
            s.append(",sc=").append(version);
        }

        version = clojureVersion();
        if (version != null) {
            s.append(",clj=").append(version);
        }

        version = groovyVersion();
        if (version != null) {
            s.append(",gy=").append(version);
        }

        version = jRubyVersion();
        if (version != null) {
            s.append(",jrb=").append(version);
        }

        return s.toString();
    }

    public static String kotlinVersion() {
        //KotlinVersion.CURRENT.toString()
        return keepMajorMinor(getStaticField("kotlin.KotlinVersion", "CURRENT"));
    }

    public static String scalaVersion() {
        // scala.util.Properties.versionNumberString()
        return keepMajorMinor(callStaticMethod("scala.util.Properties", "versionNumberString"));
    }

    public static String clojureVersion() {
        // (clojure-version) which translates to
        // clojure.core$clojure_version.invokeStatic()
        return keepMajorMinor(callStaticMethod("clojure.core$clojure_version", "invokeStatic"));
    }

    public static String groovyVersion() {
        // groovy.lang.GroovySystem.getVersion()
        // There's also getShortVersion(), but only since Groovy 3.0.1
        return keepMajorMinor(callStaticMethod("groovy.lang.GroovySystem", "getVersion"));
    }

    public static String jRubyVersion() {
        // org.jruby.runtime.Constants.VERSION
        return keepMajorMinor(getStaticField("org.jruby.runtime.Constants", "VERSION"));
    }

    private static String getStaticField(String className, String fieldName) {
        Class<?> clazz;
        try {
            clazz = Class.forName(className);
        } catch (ClassNotFoundException e) {
            return null;
        }

        try {
            Field field = clazz.getField(fieldName);
            return field.get(null).toString();
        } catch (Exception e) {
            return ""; // can't get version information
        }
    }

    private static String callStaticMethod(String className, String methodName) {
        Class<?> clazz;
        try {
            clazz = Class.forName(className);
        } catch (ClassNotFoundException e) {
            return null;
        }

        try {
            Method m = clazz.getMethod(methodName);
            return m.invoke(null).toString();
        } catch (Exception e) {
            return ""; // can't get version information
        }
    }

    static String keepMajorMinor(String version) {
        if (version == null) {
            return null;
        }

        int firstDot = version.indexOf('.');
        int secondDot = version.indexOf('.', firstDot + 1);
        if (secondDot < 0) {
            return version;
        } else {
            return version.substring(0, secondDot);
        }
    }
}
