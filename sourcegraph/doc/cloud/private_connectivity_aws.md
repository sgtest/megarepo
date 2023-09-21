# Code hosts on AWS without public access

<p>Please contact Sourcegraph directly via <a href="https://about.sourcegraph.com/contact">prefered contact method</a> for more informations</p>
</aside>

As part of the [Enterprise tier](https://about.sourcegraph.com/pricing), Sourcegraph Cloud supports connecting customer code hosts on AWS using [AWS Private Link] and managed [site-to-site VPN] solution between GCP and AWS, so that access to a private code host is secure and without the need to expose code hosts to the public internet.

## How it works

Sourcegraph Cloud is a managed service hosted on GCP. Sourcegraph creates a secure connection between customer [AWS Virtual Private Cloud] (AWS VPC) and a Sourcegraph-managed AWS account using [AWS Private Link]. Then, Sourcegraph maintains a secure connection between the Sourcegraph-managed AWS VPC and GCP Project via a managed highly available [site-to-site VPN] solution.

[link](https://link.excalidraw.com/readonly/pjmgpdt6KPHiRvXRjHj9)

<iframe src="https://link.excalidraw.com/readonly/pjmgpdt6KPHiRvXRjHj9" width="100%" height="100%" style="border: none;"></iframe>

## Steps

### Initiate the process

Customer should reach out to their account manager to initiate the process. The account manager will work with the customer to collect the required information and initiate the process, including but not limited to:

- The DNS name of the private code host, e.g., `github.internal.company.net`.
- The region of the private code host on AWS, e.g., `us-east-1`.
- The type of the TLS certificate used by the private code host, one of self-signed by internal private CA, or issued by a public CA.

### Create the VPC Endpoint Service

When a customer has private code hosts inside the AWS VPC and needs to expose it for Sourcegraph managed AWS VPC, customers can follow [AWS Documentation](https://docs.aws.amazon.com/vpc/latest/privatelink/create-endpoint-service.html). An example can be found from our [handbook](https://handbook.sourcegraph.com/departments/cloud/technical-docs/private-code-hosts/#aws-private-link-playbook-for-customer).

The customer needs to share the following detail with Sourcegraph:

- VPC endpoint serivce name in the format of `com.amazonaws.vpce.<REGION>.<VPC_ENDPOINT_SERVICE_ID>`.

Upon receiving the detail, Sourcegraph will create a connection to the customer code host, and Sourcegraph will follow up with the customer to confirm the connection is established.


### Create the code host connection

Once the connection is established, the customer can create the [code host connection](../../admin/external_service/index.md) on their Sourcegraph Cloud instance.

## FAQ

### Why AWS Private Link?

Advantages of AWS Private Link include:

- connectivity to customer VPC is only available inside AWS network
- ability to select AWS Principal (AWS Account or more granular) that can connect to customer code host
- allows customer to control incoming connections

### Why site-to-site VPN connection between GCP to AWS?

Advantages of the site-to-site GCP to AWS VPN include:

- encrypted connection between Sourcegraph Cloud and customer code host
- multiple tunnels to provide high availability between Cloud instance and customer code host

###  How can I restrict access to my private code host?

The customer has full control over the exposed service and they can may terminate the connection at any point.

[AWS Virtual Private Cloud]: https://docs.aws.amazon.com/vpc/latest/userguide/what-is-amazon-vpc.html
[AWS Private Link]: https://docs.aws.amazon.com/vpc/latest/privatelink/what-is-privatelink.html
[site-to-site VPN]: https://cloud.google.com/network-connectivity/docs/vpn/tutorials/create-ha-vpn-connections-google-cloud-aws
