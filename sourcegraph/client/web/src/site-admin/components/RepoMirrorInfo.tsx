import * as React from 'react'

import { Timestamp } from '@sourcegraph/branded/src/components/Timestamp'
import { Text, Tooltip } from '@sourcegraph/wildcard'

import { MirrorRepositoryInfoFields } from '../../graphql-operations'
import { prettyBytesBigint } from '../../util/prettyBytesBigint'

export const RepoMirrorInfo: React.FunctionComponent<
    React.PropsWithChildren<{
        mirrorInfo: MirrorRepositoryInfoFields
    }>
> = ({ mirrorInfo }) => (
    <>
        <Text className="mb-0 text-muted">
            <small>
                {mirrorInfo.updatedAt === null ? (
                    <>Not yet synced from code host.</>
                ) : (
                    <>
                        Last synced <Timestamp date={mirrorInfo.updatedAt} />. Size:{' '}
                        {prettyBytesBigint(BigInt(mirrorInfo.byteSize))}.
                        {mirrorInfo.shard !== null && <> Shard: {mirrorInfo.shard}</>}
                        {mirrorInfo.shard === null && (
                            <>
                                {' '}
                                Shard:{' '}
                                <Tooltip content="The repo has not yet been picked up by a gitserver instance.">
                                    <span>not assigned</span>
                                </Tooltip>
                            </>
                        )}
                    </>
                )}
            </small>
        </Text>
    </>
)
