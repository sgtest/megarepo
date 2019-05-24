package org.elasticsearch.gradle.info;

import org.elasticsearch.gradle.OS;
import org.gradle.api.GradleException;
import org.gradle.api.JavaVersion;
import org.gradle.api.Plugin;
import org.gradle.api.Project;
import org.gradle.api.plugins.ExtraPropertiesExtension;
import org.gradle.internal.jvm.Jvm;

import java.io.BufferedReader;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.UncheckedIOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

public class GlobalBuildInfoPlugin implements Plugin<Project> {
    private static final String GLOBAL_INFO_EXTENSION_NAME = "globalInfo";
    private static Integer _defaultParallel = null;

    @Override
    public void apply(Project project) {
        if (project != project.getRootProject()) {
            throw new IllegalStateException(this.getClass().getName() + " can only be applied to the root project.");
        }

        GlobalInfoExtension extension = project.getExtensions().create(GLOBAL_INFO_EXTENSION_NAME, GlobalInfoExtension.class);

        JavaVersion minimumCompilerVersion = JavaVersion.toVersion(getResourceContents("/minimumCompilerVersion"));
        JavaVersion minimumRuntimeVersion = JavaVersion.toVersion(getResourceContents("/minimumRuntimeVersion"));

        File compilerJavaHome = findCompilerJavaHome();
        File runtimeJavaHome = findRuntimeJavaHome(compilerJavaHome);

        final List<JavaHome> javaVersions = new ArrayList<>();
        for (int version = 8; version <= Integer.parseInt(minimumCompilerVersion.getMajorVersion()); version++) {
            if (System.getenv(getJavaHomeEnvVarName(Integer.toString(version))) != null) {
                javaVersions.add(JavaHome.of(version, new File(findJavaHome(Integer.toString(version)))));
            }
        }

        GenerateGlobalBuildInfoTask generateTask = project.getTasks().create("generateGlobalBuildInfo",
            GenerateGlobalBuildInfoTask.class, task -> {
                task.setJavaVersions(javaVersions);
                task.setMinimumCompilerVersion(minimumCompilerVersion);
                task.setMinimumRuntimeVersion(minimumRuntimeVersion);
                task.setCompilerJavaHome(compilerJavaHome);
                task.setRuntimeJavaHome(runtimeJavaHome);
                task.getOutputFile().set(new File(project.getBuildDir(), "global-build-info"));
                task.getCompilerVersionFile().set(new File(project.getBuildDir(), "java-compiler-version"));
                task.getRuntimeVersionFile().set(new File(project.getBuildDir(), "java-runtime-version"));
                task.getFipsJvmFile().set(new File(project.getBuildDir(), "in-fips-jvm"));
            });

        PrintGlobalBuildInfoTask printTask = project.getTasks().create("printGlobalBuildInfo", PrintGlobalBuildInfoTask.class, task -> {
            task.getBuildInfoFile().set(generateTask.getOutputFile());
            task.getCompilerVersionFile().set(generateTask.getCompilerVersionFile());
            task.getRuntimeVersionFile().set(generateTask.getRuntimeVersionFile());
            task.getFipsJvmFile().set(generateTask.getFipsJvmFile());
            task.setGlobalInfoListeners(extension.listeners);
        });

        project.getExtensions().getByType(ExtraPropertiesExtension.class).set("defaultParallel", findDefaultParallel(project));

        project.allprojects(p -> {
            // Make sure than any task execution generates and prints build info
            p.getTasks().all(task -> {
                if (task != generateTask && task != printTask) {
                    task.dependsOn(printTask);
                }
            });

            ExtraPropertiesExtension ext = p.getExtensions().getByType(ExtraPropertiesExtension.class);

            ext.set("compilerJavaHome", compilerJavaHome);
            ext.set("runtimeJavaHome", runtimeJavaHome);
            ext.set("isRuntimeJavaHomeSet", compilerJavaHome.equals(runtimeJavaHome) == false);
            ext.set("javaVersions", javaVersions);
            ext.set("minimumCompilerVersion", minimumCompilerVersion);
            ext.set("minimumRuntimeVersion", minimumRuntimeVersion);
            ext.set("gradleJavaVersion", Jvm.current().getJavaVersion());
        });
    }

    private static File findCompilerJavaHome() {
        String compilerJavaHome = System.getenv("JAVA_HOME");
        String compilerJavaProperty = System.getProperty("compiler.java");

        if (compilerJavaProperty != null) {
            compilerJavaHome = findJavaHome(compilerJavaProperty);
        }

        // if JAVA_HOME is not set,so we use the JDK that Gradle was run with.
        return compilerJavaHome == null ? Jvm.current().getJavaHome() : new File(compilerJavaHome);
    }

    private static File findRuntimeJavaHome(final File compilerJavaHome) {
        String runtimeJavaProperty = System.getProperty("runtime.java");

        if (runtimeJavaProperty != null) {
            return new File(findJavaHome(runtimeJavaProperty));
        }

        return System.getenv("RUNTIME_JAVA_HOME") == null ? compilerJavaHome : new File(System.getenv("RUNTIME_JAVA_HOME"));
    }

    private static String findJavaHome(String version) {
        String versionedJavaHome = System.getenv(getJavaHomeEnvVarName(version));
        if (versionedJavaHome == null) {
            throw new GradleException(
                "$versionedVarName must be set to build Elasticsearch. " +
                    "Note that if the variable was just set you might have to run `./gradlew --stop` for " +
                    "it to be picked up. See https://github.com/elastic/elasticsearch/issues/31399 details."
            );
        }
        return versionedJavaHome;
    }

    private static String getJavaHomeEnvVarName(String version) {
        return "JAVA" + version + "_HOME";
    }

    private static String getResourceContents(String resourcePath) {
        try (BufferedReader reader = new BufferedReader(
            new InputStreamReader(GlobalBuildInfoPlugin.class.getResourceAsStream(resourcePath))
        )) {
            StringBuilder b = new StringBuilder();
            for (String line = reader.readLine(); line != null; line = reader.readLine()) {
                if (b.length() != 0) {
                    b.append('\n');
                }
                b.append(line);
            }

            return b.toString();
        } catch (IOException e) {
            throw new UncheckedIOException("Error trying to read classpath resource: " + resourcePath, e);
        }
    }

    private static int findDefaultParallel(Project project) {
        // Since it costs IO to compute this, and is done at configuration time we want to cache this if possible
        // It's safe to store this in a static variable since it's just a primitive so leaking memory isn't an issue
        if (_defaultParallel == null) {
            File cpuInfoFile = new File("/proc/cpuinfo");
            if (cpuInfoFile.exists()) {
                // Count physical cores on any Linux distro ( don't count hyper-threading )
                Map<String, Integer> socketToCore = new HashMap<>();
                String currentID = "";

                try (BufferedReader reader = new BufferedReader(new FileReader(cpuInfoFile))) {
                    for (String line = reader.readLine(); line != null; line = reader.readLine()) {
                        if (line.contains(":")) {
                            List<String> parts = Arrays.stream(line.split(":", 2)).map(String::trim).collect(Collectors.toList());
                            String name = parts.get(0);
                            String value = parts.get(1);
                            // the ID of the CPU socket
                            if (name.equals("physical id")) {
                                currentID = value;
                            }
                            // Number  of cores not including hyper-threading
                            if (name.equals("cpu cores")) {
                                assert currentID.isEmpty() == false;
                                socketToCore.put("currentID", Integer.valueOf(value));
                                currentID = "";
                            }
                        }
                    }
                } catch (IOException e) {
                    throw new UncheckedIOException(e);
                }
                _defaultParallel = socketToCore.values().stream().mapToInt(i -> i).sum();
            } else if (OS.current() == OS.MAC) {
                // Ask macOS to count physical CPUs for us
                ByteArrayOutputStream stdout = new ByteArrayOutputStream();
                project.exec(spec -> {
                    spec.setExecutable("sysctl");
                    spec.args("-n", "hw.physicalcpu");
                    spec.setStandardOutput(stdout);
                });

                _defaultParallel = Integer.parseInt(stdout.toString().trim());
            }

            _defaultParallel = Runtime.getRuntime().availableProcessors() / 2;
        }

        return _defaultParallel;
    }
}
