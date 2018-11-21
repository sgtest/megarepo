import maxDate from 'date-fns/max'
import { isObject } from 'lodash'
import GithubCircleIcon from 'mdi-react/GithubCircleIcon'
import * as React from 'react'
import { RouteComponentProps } from 'react-router'
import { Link } from 'react-router-dom'
import { LinkOrSpan } from '../../../../shared/src/components/LinkOrSpan'
import { isErrorLike } from '../../../../shared/src/util/errors'
import { PageTitle } from '../../components/PageTitle'
import { Timestamp } from '../../components/time/Timestamp'
import { eventLogger } from '../../tracking/eventLogger'
import { extensionIDPrefix } from './extension'
import { ExtensionAreaRouteContext } from './ExtensionArea'
import { ExtensionREADME } from './RegistryExtensionREADME'

interface Props extends ExtensionAreaRouteContext, RouteComponentProps<{}> {}

/** A page that displays overview information about a registry extension. */
export class RegistryExtensionOverviewPage extends React.PureComponent<Props> {
    public componentDidMount(): void {
        eventLogger.logViewEvent('RegistryExtensionOverview')
    }

    public render(): JSX.Element | null {
        let repositoryURL: URL | undefined
        try {
            if (
                this.props.extension.manifest &&
                !isErrorLike(this.props.extension.manifest) &&
                this.props.extension.manifest.repository &&
                isObject(this.props.extension.manifest.repository) &&
                typeof this.props.extension.manifest.repository.url === 'string'
            ) {
                repositoryURL = new URL(this.props.extension.manifest.repository.url)
            }
        } catch (e) {
            // noop
        }

        return (
            <div className="registry-extension-overview-page row">
                <PageTitle title={this.props.extension.id} />
                <div className="col-md-8">
                    <ExtensionREADME extension={this.props.extension} />
                </div>
                <div className="col-md-4">
                    <small className="text-muted">
                        <dl className="border-top pt-2">
                            {this.props.extension.registryExtension &&
                                this.props.extension.registryExtension.publisher && (
                                    <>
                                        <dt>Publisher</dt>
                                        <dd>
                                            {this.props.extension.registryExtension.publisher ? (
                                                <Link to={this.props.extension.registryExtension.publisher.url}>
                                                    {extensionIDPrefix(
                                                        this.props.extension.registryExtension.publisher
                                                    )}
                                                </Link>
                                            ) : (
                                                'Unavailable'
                                            )}
                                        </dd>
                                    </>
                                )}
                            {this.props.extension.registryExtension &&
                                this.props.extension.registryExtension.registryName && (
                                    <>
                                        <dt
                                            className={
                                                this.props.extension.registryExtension.publisher
                                                    ? 'border-top pt-2'
                                                    : ''
                                            }
                                        >
                                            Published on
                                        </dt>
                                        <dd>
                                            <LinkOrSpan
                                                to={this.props.extension.registryExtension.remoteURL}
                                                target={
                                                    this.props.extension.registryExtension.isLocal ? undefined : '_self'
                                                }
                                            >
                                                {this.props.extension.registryExtension.registryName}
                                            </LinkOrSpan>
                                        </dd>
                                    </>
                                )}
                            <dt className="border-top pt-2">Extension ID</dt>
                            <dd>{this.props.extension.id}</dd>
                            {this.props.extension.registryExtension &&
                                (this.props.extension.registryExtension.updatedAt ||
                                    this.props.extension.registryExtension.publishedAt) && (
                                    <>
                                        <dt className="border-top pt-2">Last updated</dt>
                                        <dd>
                                            <Timestamp
                                                date={maxDate(
                                                    [
                                                        this.props.extension.registryExtension.updatedAt,
                                                        this.props.extension.registryExtension.publishedAt,
                                                    ].filter((v): v is string => !!v)
                                                )}
                                            />
                                        </dd>
                                    </>
                                )}
                            <dt className="border-top pt-2">Resources</dt>
                            <dd className="border-bottom pb-2">
                                <Link
                                    to={`${this.props.extension.registryExtension!.url}/-/manifest`}
                                    className="d-block"
                                >
                                    Manifest (package.json)
                                </Link>
                                {this.props.extension.manifest &&
                                    !isErrorLike(this.props.extension.manifest) &&
                                    this.props.extension.manifest.url && (
                                        <a
                                            href={this.props.extension.manifest.url}
                                            rel="nofollow"
                                            target="_blank"
                                            className="d-block"
                                        >
                                            Source code (JavaScript)
                                        </a>
                                    )}
                                {repositoryURL && (
                                    <div className="d-flex">
                                        {repositoryURL.hostname === 'github.com' && (
                                            <GithubCircleIcon className="icon-inline" />
                                        )}
                                        <a
                                            href={repositoryURL.href}
                                            rel="nofollow noreferrer noopener"
                                            target="_blank"
                                            className="d-block"
                                        >
                                            Repository
                                        </a>
                                    </div>
                                )}
                            </dd>
                        </dl>
                    </small>
                </div>
            </div>
        )
    }
}
