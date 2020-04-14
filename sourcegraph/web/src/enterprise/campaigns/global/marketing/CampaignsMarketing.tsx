import React from 'react'

export interface CampaignsMarketingProps {
    body: JSX.Element
}

export const CampaignsMarketing: React.FunctionComponent<CampaignsMarketingProps> = ({ body }) => (
    <>
        <section className="mt-3 mb-5">
            <h1 className="font-weight-bold display-4">
                Campaigns <span className="badge badge-info badge-outline">Beta</span>
            </h1>
            <p className="lead">
                <em>
                    Campaigns are currently in beta: during the beta period Campaigns are free to use, after the beta
                    period Campaigns will be a paid add-on. Please <a href="mailto:sales@sourcegraph.com">contact us</a>{' '}
                    for more information.
                </em>
            </p>
            <h2 className="mb-6">Make large-scale code changes across all your repositories and code hosts.</h2>
            <p className="mt-3">
                <a href="https://about.sourcegraph.com/product/code-change-management" rel="noopener">
                    Learn how
                </a>{' '}
                campaigns can help you remove legacy code, fix critical security issues, and pay down tech debt. See it
                in action to run <code>gofmt</code> over all Go repositories:
            </p>

            <div className="position-relative campaign-marketing--video-wrapper">
                <iframe
                    src="https://player.vimeo.com/video/398878670?autoplay=0&title=0&byline=0&portrait=0"
                    className="w-100 h-100 position-absolute campaign-marketing--video-frame"
                    frameBorder="0"
                    allow="autoplay; fullscreen"
                    allowFullScreen={true}
                />
            </div>
        </section>

        {body}

        <section className="py-5">
            <h2>Share your feedback</h2>
            <p>
                Get in touch on Twitter <a href="https://twitter.com/srcgraph">@srcgraph</a>, file an issue in our{' '}
                <a href="https://github.com/sourcegraph/sourcegraph/issues">public issue tracker</a>, or email{' '}
                <a href="mailto:feedback@sourcegraph.com">feedback@sourcegraph.com</a> to share your feedback and use
                cases. We look forward to hearing from you!
            </p>
        </section>
    </>
)
