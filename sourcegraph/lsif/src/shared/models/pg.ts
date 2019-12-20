import { Column, Entity, JoinColumn, OneToOne, PrimaryGeneratedColumn } from 'typeorm'
import { EncodedBloomFilter } from '../datastructures/bloom-filter'
import { MAX_POSTGRES_BATCH_SIZE } from '../constants'

/**
 * The possible states of an LsifUpload entity.
 */
export type LsifUploadState = 'queued' | 'completed' | 'errored' | 'processing'

/**
 * An entity within Postgres. This entity carries the data necessary to convert an
 * LSIF upload out-of-band, and hold metadata about the conversion process once it
 * completes (or fails). These entities are not meant to exist indefinitely and are
 * removed from the table based on their age.
 */
@Entity({ name: 'lsif_uploads' })
export class LsifUpload {
    /**
     * The number of model instances that can be inserted at once.
     */
    public static BatchSize = MAX_POSTGRES_BATCH_SIZE

    /**
     * A unique ID required by typeorm entities.
     */
    @PrimaryGeneratedColumn('increment', { type: 'int' })
    public id!: number

    /**
     *  The name of the source repository.
     */
    @Column('text')
    public repository!: string

    /**
     *  The source commit.
     */
    @Column('text')
    public commit!: string

    /**
     *  The path at which this LSIF dump is mounted.
     */
    @Column('text')
    public root!: string

    /**
     * The temporary file the upload data is stored. As the file is deleted after
     * it has been converted, this value is only meaningful while the upload has
     * state `queued` or `processing`.
     */
    @Column('text')
    public filename!: string

    /**
     * The conversion state of the upload. May be `queued`, `processing`, `completed`,
     * or `errored`.
     */
    @Column('text')
    public state!: LsifUploadState

    /**
     * The error message that occurred during processing (if any).
     */
    @Column('text', { name: 'failure_summary', nullable: true })
    public failureSummary!: string

    /**
     * The stacktrace of the error that occurred during processing (if any).
     */
    @Column('text', { name: 'failure_stacktrace', nullable: true })
    public failureStacktrace!: string

    /**
     * The time the dump was uploaded.
     */
    @Column('timestamp with time zone', { name: 'uploaded_at' })
    public uploadedAt!: Date

    /**
     * The time the upload started its conversion.
     */
    @Column('timestamp with time zone', { name: 'started_at', nullable: true })
    public startedAt!: Date

    /**
     * The time the conversion completed or errored.
     */
    @Column('timestamp with time zone', { name: 'finished_at', nullable: true })
    public finishedAt!: Date

    /**
     * The opentracing headers from the upload request.
     */
    @Column('text', { name: 'tracing_context' })
    public tracingContext!: string
}

/**
 * An entity within Postgres. This tracks commit parentage and branch heads for all
 * known repositories.
 */
@Entity({ name: 'lsif_commits' })
export class Commit {
    /**
     * The number of model instances that can be inserted at once.
     */
    public static BatchSize = MAX_POSTGRES_BATCH_SIZE

    /**
     * A unique ID required by typeorm entities.
     */
    @PrimaryGeneratedColumn('increment', { type: 'int' })
    public id!: number

    /**
     * The name of the source repository.
     */
    @Column('text')
    public repository!: string

    /**
     * The source commit.
     */
    @Column('text')
    public commit!: string

    /**
     * A parent commit. Multiple parents are represented by distinct rows
     * with the same `repository` and `commit`` fields. This value is an
     * empty string for a commit with no parent.
     */
    @Column('text', { name: 'parent_commit', nullable: true })
    public parentCommit!: string | null
}

/**
 * The primary key of the `lsif_dumps` table.
 */
export type DumpId = number

/**
 * An entity within Postgres. A row with a repository and commit indicates that
 * there exists LSIF data for that pair.
 */
@Entity({ name: 'lsif_dumps' })
export class LsifDump {
    /**
     * The number of model instances that can be inserted at once.
     */
    public static BatchSize = MAX_POSTGRES_BATCH_SIZE

    /**
     * A unique ID required by typeorm entities.
     */
    @PrimaryGeneratedColumn('increment', { type: 'int' })
    public id!: DumpId

    /**
     * The name of the source repository.
     */
    @Column('text')
    public repository!: string

    /**
     * The source commit.
     */
    @Column('text')
    public commit!: string

    /**
     * The path at which this LSIF dump is mounted. Roots for two different LSIF
     * dumps at the same repo@commit must not overlap (they are not allowed be
     * prefixes of one another). Defaults to the empty string, which indicates
     * this is the only LSIF dump for the repo@commit.
     */
    @Column('text')
    public root!: string

    /**
     * Whether or not this commit is visible at the tip of the default branch.
     */
    @Column('boolean', { name: 'visible_at_tip' })
    public visibleAtTip!: boolean

    /**
     * The time the dump was uploaded.
     */
    @Column('timestamp with time zone', { name: 'uploaded_at' })
    public uploadedAt!: Date

    /**
     * The time the dump was created.
     */
    @Column('timestamp with time zone', { name: 'processed_at' })
    public processedAt!: Date
}

/**
 * The base class for `PackageModel` and `ReferenceModel` as they have nearly
 * identical column descriptions.
 */
class Package {
    /**
     * A unique ID required by typeorm entities.
     */
    @PrimaryGeneratedColumn('increment', { type: 'int' })
    public id!: number

    /**
     * The name of the package type (e.g. npm, pip).
     */
    @Column('text')
    public scheme!: string

    /**
     * The name of the package this repository and commit provides.
     */
    @Column('text')
    public name!: string

    /**
     * The version of the package this repository and commit provides.
     */
    @Column('text', { nullable: true })
    public version!: string | null

    /**
     * The corresponding dump, `LsifDump` when querying and `DumpId` when
     * inserting.
     */
    @OneToOne(() => LsifDump, { eager: true })
    @JoinColumn({ name: 'dump_id' })
    public dump!: LsifDump

    /**
     * The foreign key to the dump.
     */
    @Column('integer')
    public dump_id!: DumpId
}

/**
 * An entity within Postgres. This maps a given repository and commit pair to the package
 * that it provides to other projects.
 */
@Entity({ name: 'lsif_packages' })
export class PackageModel extends Package {
    /**
     * The number of model instances that can be inserted at once.
     */
    public static BatchSize = MAX_POSTGRES_BATCH_SIZE
}

/**
 * An entity within Postgres. This lists the dependencies of a given repository and commit
 * pair to support find global reference operations.
 */
@Entity({ name: 'lsif_references' })
export class ReferenceModel extends Package {
    /**
     * The number of model instances that can be inserted at once.
     */
    public static BatchSize = MAX_POSTGRES_BATCH_SIZE

    /**
     * A serialized bloom filter that encodes the set of symbols that this repository
     * and commit imports from the given package. Testing this filter will prevent
     * the backend from opening databases that will yield no results for a particular
     * symbol.
     */
    @Column('bytea')
    public filter!: EncodedBloomFilter
}

/**
 * The entities composing the Postgres database models.
 */
export const entities = [LsifUpload, Commit, LsifDump, PackageModel, ReferenceModel]
