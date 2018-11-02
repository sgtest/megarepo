package org.elasticsearch.gradle;

import java.io.IOException;
import java.io.InputStream;
import java.util.HashMap;
import java.util.Map;
import java.util.Properties;

/**
 * Accessor for shared dependency versions used by elasticsearch, namely the elasticsearch and lucene versions.
 */
public class VersionProperties {
    public static String getElasticsearch() {
        return elasticsearch;
    }

    public static String getLucene() {
        return lucene;
    }

    public static Map<String, String> getVersions() {
        return versions;
    }

    private static final String elasticsearch;
    private static final String lucene;
    private static final Map<String, String> versions = new HashMap<String, String>();
    static {
        Properties props = getVersionProperties();
        elasticsearch = props.getProperty("elasticsearch");
        lucene = props.getProperty("lucene");
        for (String property : props.stringPropertyNames()) {
            versions.put(property, props.getProperty(property));
        }
    }

    private static Properties getVersionProperties() {
        Properties props = new Properties();
        InputStream propsStream = VersionProperties.class.getResourceAsStream("/version.properties");
        if (propsStream == null) {
            throw new IllegalStateException("/version.properties resource missing");
        }
        try {
            props.load(propsStream);
        } catch (IOException e) {
            throw new IllegalStateException("Failed to load version properties", e);
        }
        return props;
    }

    public static boolean isElasticsearchSnapshot() {
        return elasticsearch.endsWith("-SNAPSHOT");
    }
}
