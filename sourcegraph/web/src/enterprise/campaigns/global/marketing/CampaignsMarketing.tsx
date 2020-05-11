import React from 'react'

export interface CampaignsMarketingProps {
    body: JSX.Element
}

export const CampaignsMarketing: React.FunctionComponent<CampaignsMarketingProps> = ({ body }) => (
    <>
        <section className="mt-3 mb-5">
            <h1 className="font-weight-bold display-4">
                Campaigns <span className="badge badge-info">Beta</span>
            </h1>
            <h2 className="mb-5">Make and track large-scale changes across all code</h2>

            <div className="text-center">
                <iframe
                    width="560"
                    height="315"
                    src="https://www.youtube.com/embed/aqcCrqRB17w"
                    frameBorder="0"
                    allow="accelerometer; autoplay; encrypted-media; gyroscope; picture-in-picture"
                    allowFullScreen={true}
                />
            </div>
        </section>

        {body}

        <section className="my-3">
            <h2>Ask questions and share feedback</h2>
            <p>
                Get in touch on Twitter <a href="https://twitter.com/srcgraph">@srcgraph</a>, file an issue in our{' '}
                <a href="https://github.com/sourcegraph/sourcegraph/issues">public issue tracker</a>, or email{' '}
                <a href="mailto:feedback@sourcegraph.com">feedback@sourcegraph.com</a>. We look forward to hearing from
                you!
            </p>
        </section>
    </>
)
