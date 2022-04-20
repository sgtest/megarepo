import React, { useMemo } from 'react'

import { mapValues, values } from 'lodash'
import { RouteComponentProps } from 'react-router'

import { ExternalServiceKind } from '@sourcegraph/shared/src/graphql-operations'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { LoadingSpinner, useObservable, Alert, Link } from '@sourcegraph/wildcard'

import awsCodeCommitJSON from '../../../../schema/aws_codecommit.schema.json'
import bitbucketCloudSchemaJSON from '../../../../schema/bitbucket_cloud.schema.json'
import bitbucketServerSchemaJSON from '../../../../schema/bitbucket_server.schema.json'
import gerritSchemaJSON from '../../../../schema/gerrit.schema.json'
import githubSchemaJSON from '../../../../schema/github.schema.json'
import gitlabSchemaJSON from '../../../../schema/gitlab.schema.json'
import gitoliteSchemaJSON from '../../../../schema/gitolite.schema.json'
import goModulesSchemaJSON from '../../../../schema/go-modules.schema.json'
import jvmPackagesSchemaJSON from '../../../../schema/jvm-packages.schema.json'
import npmPackagesSchemaJSON from '../../../../schema/npm-packages.schema.json'
import otherExternalServiceSchemaJSON from '../../../../schema/other_external_service.schema.json'
import pagureSchemaJSON from '../../../../schema/pagure.schema.json'
import perforceSchemaJSON from '../../../../schema/perforce.schema.json'
import phabricatorSchemaJSON from '../../../../schema/phabricator.schema.json'
import settingsSchemaJSON from '../../../../schema/settings.schema.json'
import siteSchemaJSON from '../../../../schema/site.schema.json'
import { PageTitle } from '../components/PageTitle'
import { DynamicallyImportedMonacoSettingsEditor } from '../settings/DynamicallyImportedMonacoSettingsEditor'

import { fetchAllConfigAndSettings, fetchMonitoringStats } from './backend'

/**
 * Minimal shape of a JSON Schema. These values are treated as opaque, so more specific types are
 * not needed.
 */
interface JSONSchema {
    $id: string
    definitions?: Record<string, { type: string }>
}

const externalServices: Record<ExternalServiceKind, JSONSchema> = {
    AWSCODECOMMIT: awsCodeCommitJSON,
    BITBUCKETCLOUD: bitbucketCloudSchemaJSON,
    BITBUCKETSERVER: bitbucketServerSchemaJSON,
    GERRIT: gerritSchemaJSON,
    GITHUB: githubSchemaJSON,
    GITLAB: gitlabSchemaJSON,
    GITOLITE: gitoliteSchemaJSON,
    GOMODULES: goModulesSchemaJSON,
    JVMPACKAGES: jvmPackagesSchemaJSON,
    NPMPACKAGES: npmPackagesSchemaJSON,
    OTHER: otherExternalServiceSchemaJSON,
    PERFORCE: perforceSchemaJSON,
    PHABRICATOR: phabricatorSchemaJSON,
    PAGURE: pagureSchemaJSON,
}

const allConfigSchema = {
    $id: 'all.schema.json#',
    allowComments: true,
    additionalProperties: false,
    properties: {
        site: siteSchemaJSON,
        externalServices: {
            type: 'object',
            properties: mapValues(externalServices, schema => ({ type: 'array', items: schema })),
        },
        settings: {
            type: 'object',
            properties: {
                subjects: {
                    type: 'array',
                    items: {
                        type: 'object',
                        properties: {
                            __typename: {
                                type: 'string',
                            },
                            settingsURL: {
                                type: ['string', 'null'],
                            },
                            contents: {
                                ...settingsSchemaJSON,
                                type: ['object', 'null'],
                            },
                        },
                    },
                },
                final: settingsSchemaJSON,
            },
        },
        alerts: {
            type: 'array',
            items: {
                type: 'object',
            },
        },
    },
    definitions: values(externalServices)
        .map(schema => schema.definitions)
        .concat([siteSchemaJSON.definitions, settingsSchemaJSON.definitions])
        .reduce((allDefinitions, definitions) => ({ ...allDefinitions, ...definitions }), {}),
}

interface Props extends RouteComponentProps, ThemeProps, TelemetryProps {}

export const SiteAdminReportBugPage: React.FunctionComponent<Props> = ({ isLightTheme, telemetryService, history }) => {
    const monitoringDaysBack = 7
    const monitoringStats = useObservable(useMemo(() => fetchMonitoringStats(monitoringDaysBack), []))
    const allConfig = useObservable(useMemo(fetchAllConfigAndSettings, []))
    return (
        <div>
            <PageTitle title="Report a bug - Admin" />
            <h2>Report a bug</h2>
            <p>
                <Link
                    target="_blank"
                    rel="noopener noreferrer"
                    to="https://github.com/sourcegraph/sourcegraph/issues/new?assignees=&labels=&template=bug_report.md&title="
                >
                    Create an issue on the public issue tracker
                </Link>
                , and include a description of the bug along with the info below (with secrets redacted). If the report
                contains sensitive information that should not be public, email the report to{' '}
                <Link target="_blank" rel="noopener noreferrer" to="mailto:support@sourcegraph.com">
                    support@sourcegraph.com
                </Link>{' '}
                instead.
            </p>
            <Alert variant="warning">
                <div>
                    Please redact any secrets before sharing, whether on the public issue tracker or with
                    support@sourcegraph.com.
                </div>
            </Alert>
            {allConfig === undefined || monitoringStats === undefined ? (
                <LoadingSpinner className="mt-2" />
            ) : (
                <DynamicallyImportedMonacoSettingsEditor
                    value={JSON.stringify(
                        monitoringStats ? { ...allConfig, ...monitoringStats } : { ...allConfig, alerts: null },
                        undefined,
                        2
                    )}
                    jsonSchema={allConfigSchema}
                    canEdit={false}
                    height={800}
                    isLightTheme={isLightTheme}
                    history={history}
                    readOnly={true}
                    telemetryService={telemetryService}
                />
            )}
        </div>
    )
}
