import FileDownloadIcon from 'mdi-react/FileDownloadIcon'
import React, { useMemo } from 'react'
import { Link } from '../../../../../shared/src/components/Link'
import { CodeSnippet } from '../../../components/CodeSnippet'
import { Timestamp } from '../../../components/time/Timestamp'
import { CampaignFields } from '../../../graphql-operations'

export interface CampaignSpecTabProps {
    campaign: Pick<CampaignFields, 'name' | 'createdAt' | 'lastApplier' | 'lastAppliedAt'>
    originalInput: CampaignFields['currentSpec']['originalInput']
}

/** Reports whether str is a valid JSON document. */
const isJSON = (string: string): boolean => {
    try {
        JSON.parse(string)
        return true
    } catch {
        return false
    }
}

export const CampaignSpecTab: React.FunctionComponent<CampaignSpecTabProps> = ({
    campaign: { name: campaignName, createdAt, lastApplier, lastAppliedAt },
    originalInput,
}) => {
    const downloadUrl = useMemo(() => 'data:text/plain;charset=utf-8,' + encodeURIComponent(originalInput), [
        originalInput,
    ])

    // JSON is valid YAML, so the input might be JSON. In that case, we'll highlight and indent it
    // as JSON. This is especially nice when the input is a "minified" (no extraneous whitespace)
    // JSON document that's difficult to read unless indented.
    const inputIsJSON = isJSON(originalInput)
    const input = useMemo(() => (inputIsJSON ? JSON.stringify(JSON.parse(originalInput), null, 2) : originalInput), [
        inputIsJSON,
        originalInput,
    ])

    return (
        <>
            <div className="d-flex justify-content-between align-items-center mb-2 test-campaigns-spec">
                <p className="m-0">
                    {lastApplier ? <Link to={lastApplier.url}>{lastApplier.username}</Link> : 'A deleted user'}{' '}
                    {createdAt === lastAppliedAt ? 'created' : 'updated'} this campaign{' '}
                    <Timestamp date={lastAppliedAt} /> by applying the following campaign spec:
                </p>
                <a
                    download={`${campaignName}.campaign.yaml`}
                    href={downloadUrl}
                    className="text-right btn btn-secondary text-nowrap"
                    data-tooltip={`Download ${campaignName}.campaign.yaml`}
                >
                    <FileDownloadIcon className="icon-inline" /> Download YAML
                </a>
            </div>
            <CodeSnippet code={input} language={inputIsJSON ? 'json' : 'yaml'} className="mb-3" />
        </>
    )
}
