/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.index.translog;

import joptsimple.OptionParser;
import joptsimple.OptionSet;
import joptsimple.OptionSpec;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.IndexCommit;
import org.apache.lucene.index.IndexWriter;
import org.apache.lucene.index.IndexWriterConfig;
import org.apache.lucene.index.NoMergePolicy;
import org.apache.lucene.store.Directory;
import org.apache.lucene.store.FSDirectory;
import org.apache.lucene.store.Lock;
import org.apache.lucene.store.LockObtainFailedException;
import org.apache.lucene.store.NativeFSLockFactory;
import org.apache.lucene.store.OutputStreamDataOutput;
import org.elasticsearch.core.internal.io.IOUtils;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.cli.EnvironmentAwareCommand;
import org.elasticsearch.cli.Terminal;
import org.elasticsearch.common.SuppressForbidden;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.io.PathUtils;
import org.elasticsearch.env.Environment;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.index.engine.Engine;
import org.elasticsearch.index.seqno.SequenceNumbers;

import java.io.IOException;
import java.nio.channels.Channels;
import java.nio.channels.FileChannel;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

public class TruncateTranslogCommand extends EnvironmentAwareCommand {

    private final OptionSpec<String> translogFolder;
    private final OptionSpec<Void> batchMode;

    public TruncateTranslogCommand() {
        super("Truncates a translog to create a new, empty translog");
        this.translogFolder = parser.acceptsAll(Arrays.asList("d", "dir"),
                "Translog Directory location on disk")
                .withRequiredArg()
                .required();
        this.batchMode = parser.acceptsAll(Arrays.asList("b", "batch"),
                "Enable batch mode explicitly, automatic confirmation of warnings");
    }

    // Visible for testing
    public OptionParser getParser() {
        return this.parser;
    }

    @Override
    protected void printAdditionalHelp(Terminal terminal) {
        terminal.println("This tool truncates the translog and translog");
        terminal.println("checkpoint files to create a new translog");
    }

    @SuppressForbidden(reason = "Necessary to use the path passed in")
    private Path getTranslogPath(OptionSet options) {
        return PathUtils.get(translogFolder.value(options), "", "");
    }

    @Override
    protected void execute(Terminal terminal, OptionSet options, Environment env) throws Exception {
        boolean batch = options.has(batchMode);

        Path translogPath = getTranslogPath(options);
        Path idxLocation = translogPath.getParent().resolve("index");

        if (Files.exists(translogPath) == false || Files.isDirectory(translogPath) == false) {
            throw new ElasticsearchException("translog directory [" + translogPath + "], must exist and be a directory");
        }

        if (Files.exists(idxLocation) == false || Files.isDirectory(idxLocation) == false) {
            throw new ElasticsearchException("unable to find a shard at [" + idxLocation + "], which must exist and be a directory");
        }
        try (Directory dir = FSDirectory.open(idxLocation, NativeFSLockFactory.INSTANCE)) {
            final String historyUUID = UUIDs.randomBase64UUID();
            final Map<String, String> commitData;
            // Hold the lock open for the duration of the tool running
            try (Lock writeLock = dir.obtainLock(IndexWriter.WRITE_LOCK_NAME)) {
                Set<Path> translogFiles;
                try {
                    terminal.println("Checking existing translog files");
                    translogFiles = filesInDirectory(translogPath);
                } catch (IOException e) {
                    terminal.println("encountered IOException while listing directory, aborting...");
                    throw new ElasticsearchException("failed to find existing translog files", e);
                }

                // Warn about ES being stopped and files being deleted
                warnAboutDeletingFiles(terminal, translogFiles, batch);

                List<IndexCommit> commits;
                try {
                    terminal.println("Reading translog UUID information from Lucene commit from shard at [" + idxLocation + "]");
                    commits = DirectoryReader.listCommits(dir);
                } catch (IndexNotFoundException infe) {
                    throw new ElasticsearchException("unable to find a valid shard at [" + idxLocation + "]", infe);
                }

                // Retrieve the generation and UUID from the existing data
                commitData = new HashMap<>(commits.get(commits.size() - 1).getUserData());
                String translogGeneration = commitData.get(Translog.TRANSLOG_GENERATION_KEY);
                String translogUUID = commitData.get(Translog.TRANSLOG_UUID_KEY);
                final long globalCheckpoint;
                // In order to have a safe commit invariant, we have to assign the global checkpoint to the max_seqno of the last commit.
                // We can only safely do it because we will generate a new history uuid this shard.
                if (commitData.containsKey(SequenceNumbers.MAX_SEQ_NO)) {
                    globalCheckpoint = Long.parseLong(commitData.get(SequenceNumbers.MAX_SEQ_NO));
                    // Also advances the local checkpoint of the last commit to its max_seqno.
                    commitData.put(SequenceNumbers.LOCAL_CHECKPOINT_KEY, Long.toString(globalCheckpoint));
                } else {
                    globalCheckpoint = SequenceNumbers.UNASSIGNED_SEQ_NO;
                }
                if (translogGeneration == null || translogUUID == null) {
                    throw new ElasticsearchException("shard must have a valid translog generation and UUID but got: [{}] and: [{}]",
                        translogGeneration, translogUUID);
                }
                terminal.println("Translog Generation: " + translogGeneration);
                terminal.println("Translog UUID      : " + translogUUID);
                terminal.println("History UUID      : " + historyUUID);

                Path tempEmptyCheckpoint = translogPath.resolve("temp-" + Translog.CHECKPOINT_FILE_NAME);
                Path realEmptyCheckpoint = translogPath.resolve(Translog.CHECKPOINT_FILE_NAME);
                Path tempEmptyTranslog = translogPath.resolve("temp-" + Translog.TRANSLOG_FILE_PREFIX +
                    translogGeneration + Translog.TRANSLOG_FILE_SUFFIX);
                Path realEmptyTranslog = translogPath.resolve(Translog.TRANSLOG_FILE_PREFIX +
                    translogGeneration + Translog.TRANSLOG_FILE_SUFFIX);

                // Write empty checkpoint and translog to empty files
                long gen = Long.parseLong(translogGeneration);
                int translogLen = writeEmptyTranslog(tempEmptyTranslog, translogUUID);
                writeEmptyCheckpoint(tempEmptyCheckpoint, translogLen, gen, globalCheckpoint);

                terminal.println("Removing existing translog files");
                IOUtils.rm(translogFiles.toArray(new Path[]{}));

                terminal.println("Creating new empty checkpoint at [" + realEmptyCheckpoint + "]");
                Files.move(tempEmptyCheckpoint, realEmptyCheckpoint, StandardCopyOption.ATOMIC_MOVE);
                terminal.println("Creating new empty translog at [" + realEmptyTranslog + "]");
                Files.move(tempEmptyTranslog, realEmptyTranslog, StandardCopyOption.ATOMIC_MOVE);

                // Fsync the translog directory after rename
                IOUtils.fsync(translogPath, true);
            }

            terminal.println("Marking index with the new history uuid");
            // commit the new histroy id
            IndexWriterConfig iwc = new IndexWriterConfig(null)
                .setCommitOnClose(false)
                // we don't want merges to happen here - we call maybe merge on the engine
                // later once we stared it up otherwise we would need to wait for it here
                // we also don't specify a codec here and merges should use the engines for this index
                .setMergePolicy(NoMergePolicy.INSTANCE)
                .setOpenMode(IndexWriterConfig.OpenMode.APPEND);
            try (IndexWriter writer = new IndexWriter(dir, iwc)) {
                Map<String, String> newCommitData = new HashMap<>(commitData);
                newCommitData.put(Engine.HISTORY_UUID_KEY, historyUUID);
                writer.setLiveCommitData(newCommitData.entrySet());
                writer.commit();
            }
        } catch (LockObtainFailedException lofe) {
            throw new ElasticsearchException("Failed to lock shard's directory at [" + idxLocation + "], is Elasticsearch still running?");
        }

        terminal.println("Done.");
    }

    /** Write a checkpoint file to the given location with the given generation */
    static void writeEmptyCheckpoint(Path filename, int translogLength, long translogGeneration, long globalCheckpoint) throws IOException {
        Checkpoint emptyCheckpoint = Checkpoint.emptyTranslogCheckpoint(translogLength, translogGeneration,
            globalCheckpoint, translogGeneration);
        Checkpoint.write(FileChannel::open, filename, emptyCheckpoint,
            StandardOpenOption.WRITE, StandardOpenOption.READ, StandardOpenOption.CREATE_NEW);
        // fsync with metadata here to make sure.
        IOUtils.fsync(filename, false);
    }

    /**
     * Write a translog containing the given translog UUID to the given location. Returns the number of bytes written.
     */
    public static int writeEmptyTranslog(Path filename, String translogUUID) throws IOException {
        try (FileChannel fc = FileChannel.open(filename, StandardOpenOption.WRITE, StandardOpenOption.CREATE_NEW)) {
            TranslogHeader header = new TranslogHeader(translogUUID, TranslogHeader.UNKNOWN_PRIMARY_TERM);
            header.write(fc);
            return header.sizeInBytes();
        }
    }

    /** Show a warning about deleting files, asking for a confirmation if {@code batchMode} is false */
    public static void warnAboutDeletingFiles(Terminal terminal, Set<Path> files, boolean batchMode) {
        terminal.println("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
        terminal.println("!   WARNING: Elasticsearch MUST be stopped before running this tool   !");
        terminal.println("!                                                                     !");
        terminal.println("!   WARNING:    Documents inside of translog files will be lost       !");
        terminal.println("!                                                                     !");
        terminal.println("!   WARNING:          The following files will be DELETED!            !");
        terminal.println("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
        for (Path file : files) {
            terminal.println("--> " + file);
        }
        terminal.println("");
        if (batchMode == false) {
            String text = terminal.readText("Continue and DELETE files? [y/N] ");
            if (!text.equalsIgnoreCase("y")) {
                throw new ElasticsearchException("aborted by user");
            }
        }
    }

    /** Return a Set of all files in a given directory */
    public static Set<Path> filesInDirectory(Path directory) throws IOException {
        Set<Path> files = new HashSet<>();
        try (DirectoryStream<Path> stream = Files.newDirectoryStream(directory)) {
            for (Path file : stream) {
                files.add(file);
            }
        }
        return files;
    }

}
