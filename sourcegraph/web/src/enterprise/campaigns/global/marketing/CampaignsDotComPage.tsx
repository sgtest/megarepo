import React from 'react'
import { PrivateCodeCta } from '../../../../search/input/PrivateCodeCta'
import { CampaignsFlushEdgesIcon } from '../../icons'
import { PageHeader } from '../../../../components/PageHeader'

export interface CampaignsDotComPageProps {
    // Nothing for now.
}

export const CampaignsDotComPage: React.FunctionComponent<CampaignsDotComPageProps> = () => (
    <div className="container web-content mt-3">
        <section className="mt-3 mb-5">
            <PageHeader
                icon={CampaignsFlushEdgesIcon}
                title={
                    <>
                        Campaigns{' '}
                        <sup>
                            <span className="badge badge-merged text-uppercase">Beta</span>
                        </sup>
                    </>
                }
            />
            <h2 className="mb-5">Make and track large-scale changes across all code</h2>

            <div className="text-center">
                <iframe
                    className="percy-hide chromatic-ignore"
                    width="560"
                    height="315"
                    src="https://www.youtube.com/embed/EfKwKFzOs3E"
                    frameBorder="0"
                    allow="accelerometer; autoplay; encrypted-media; gyroscope; picture-in-picture"
                    allowFullScreen={true}
                />
            </div>
        </section>

        <div className="row">
            <section className="my-3 col-md-8 col-xs-12">
                <h2>Get started</h2>
                <p>
                    <strong>Campaigns are not available on Sourcegraph.com</strong>. Instead, use a private Sourcegraph
                    instance to try them on your code.
                </p>
                <ol>
                    <li>
                        Install a private Sourcegraph instance using the{' '}
                        <a href="https://docs.sourcegraph.com/#quickstart-guide" rel="noopener">
                            quickstart guide.
                        </a>
                    </li>
                    <li>
                        <a href="https://docs.sourcegraph.com/admin/repo/add">Add repositories</a> from your code host
                        to Sourcegraph.
                    </li>
                    <li>
                        Follow the{' '}
                        <a href="https://docs.sourcegraph.com/user/campaigns/getting_started" rel="noopener">
                            Getting started with campaigns
                        </a>{' '}
                        guide to enable campaigns on your instance and start using them.
                    </li>
                </ol>

                <p>
                    Learn more about campaigns{' '}
                    <a href="https://docs.sourcegraph.com/user/campaigns" rel="noopener">
                        in the documentation
                    </a>
                    .
                </p>
                <section className="my-3">
                    <h2>Ask questions and share feedback</h2>
                    <p>
                        Get in touch on Twitter <a href="https://twitter.com/srcgraph">@srcgraph</a>, file an issue in
                        our <a href="https://github.com/sourcegraph/sourcegraph/issues">public issue tracker</a>, or
                        email <a href="mailto:feedback@sourcegraph.com">feedback@sourcegraph.com</a>. We look forward to
                        hearing from you!
                    </p>
                </section>
            </section>
            <div className="offset-md-1 col-md-10 offset-lg-0 col-lg-4">
                <PrivateCodeCta />
            </div>
        </div>
    </div>
)
