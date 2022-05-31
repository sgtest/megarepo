# SAML
Security Assertion Markup Language (SAML) is a common web protocol used to pass authorized credentials between two web applications, a service provider (SP) - Sourcegraph in this instance and an Identity Provider (IdP). This communication is conducted via XML assertions.

## Identity Providers

Select your SAML identity provider for setup instructions:

- [Okta](okta.md)
- [Azure Active Directory (Azure AD)](azure_ad.md)
- [Microsoft Active Directory Federation Services (ADFS)](microsoft_adfs.md)
- [Auth0](generic.md)
- [OneLogin](one_login.md)
- [Ping Identity](generic.md)
- [Salesforce Identity](generic.md)
- [JumpCloud](jump_cloud.md)
- [Other](generic.md)

For advanced SAML configuration options, see the [`saml` auth provider documentation](../../config/site_config.md#saml).

> NOTE: Sourcegraph currently supports at most 1 SAML auth provider at a time (but you can configure additional auth providers of other types). This should not be an issue for 99% of customers.

## Add a SAML provider

1. In Sourcegraph [site config](../../config/site_config.md), ensure `externalURL` is set to a value consistent with the URL you used in the previous section in the identity provider configuration.

    > NOTE: Make sure to use the exact same scheme (`http` or `https`), and there should be no trailing slash.

2. Add an item to `auth.providers` with `type` "saml" and *either* `identityProviderMetadataURL` or `identityProviderMetadata` set. The former is preferred, but not all identity providers support it (it is sometimes called "App Federation Metadata URL" or just "SAML metadata URL").

    > WARNING: There can only be at most 1 element of type `saml` in `auth.providers`. Otherwise behavior is undefined. If you have another SAML auth provider configured, remove it from `auth.providers` before proceeding.

Here are some examples of what your site config might look like:

- Example 1:

  ```json
  {
    // ...
    "externalURL": "https://sourcegraph.example.com",
    "auth.providers": [
      {
        "type": "saml",
        "configID": "generic",
        "identityProviderMetadataURL": "https://example.com/saml-metadata"
      }
    ]
  }
  ```

- Example 2:

  ```json
  {
    // ...
    "externalURL": "https://sourcegraph.example.com",
    "auth.providers": [
      {
        "type": "saml",
        "configID": "generic",

        // This is a long XML string you download from your identity provider.
        // You can escape it to a JSON string using a tool like
        // https://json-escape-text.now.sh.
        "identityProviderMetadata": "<?xml version=\"1.0\" encoding=\"utf-8\"?><EntityDescriptor ID=\"_86c6d3fd-e0a9-4b99-b830-40b248003fb9\" entityID=\"https://sts.windows.net/6c1b91af-8e37-4921-bbfa-ef68aa2e2d1e/\" xmlns=\"urn:oasis:names:tc:SAML:2.0:metadata\"><Signature xmlns=\"http://www.w3.org/2000/09/xmldsig#\"><SignedInfo><CanonicalizationMethod Algorithm=\"http://www.w3.org/2001/10/xml-exc-c14n#\" /><SignatureMethod Algorithm=\"http://www.w3.org/2001/04/xmldsig-more#rsa-sha256\" /><Reference URI=\"#_86c6d3fd-e0a9-4b99-b830-40b248003fb9\"><Transforms><Transform Algorithm=\"http://www.w3.org/2000/09/xmldsig#enveloped-signature\" /><Transform Algorithm=\"http://www.w3.org/2001/10/xml-exc-c14n#\" /></Transforms><DigestMethod Algorithm=\"http://www.w3.org/2001/04/xmlenc#sha256\" /><DigestValue> ..."
      }
    ]
  }
  ```

Then, confirm that there are no error messages in:

- The `sourcegraph-frontend` deployment logs for instances using [Docker Compose](../../deploy/docker-compose/index.md) and [Kubernetes](../../deploy/kubernetes/index.md)
- The `sourcegraph/server` container logs for instances using a [single docker container](../../deploy/docker-single-container/index.md)

The most likely error message indicating a problem is:

```
Error prefetching SAML service provider metadata
```

See [SAML troubleshooting](#troubleshooting) for more tips.

## Troubleshooting

### Enable logging in Sourcegraph containers
Set the env var `INSECURE_SAML_LOG_TRACES=1` to log all SAML requests and responses on:

- The `sourcegraph-frontend` deployment for instances using [Docker Compose](../../deploy/docker-compose/index.md) and [Kubernetes](../../deploy/kubernetes/index.md)
- The `sourcegraph/server` container for instances using a [single docker container](../../deploy/docker-single-container/index.md)

### Debugging with your browser
When debugging a problem with SAML its often helpful to use the browser's developer tools to directly observe the XML assertions and their contents. Below are some general pointers on how to collect SAML communications:

1. Navigate to Sourcegraph in the browser and prepare to attempt a login via SAML
2. Open the developer tools and navigate to the `Network` tab and enable the option to preserve logs if it is available
3. Clear the collection of network logs in the the `Network` tab and attempt a SAML login
4. Look for a network request in the `Network` tab that indicates a SAML request response communication (this might be labeled ACS, or Authn)
5. Select the network request and observe its headers

You should see something like the image below from a Sourcegraph Okta login, observed via Safari devTools:

![Screen Shot 2021-09-15 at 1 13 17 PM](https://user-images.githubusercontent.com/13024338/134255811-88250622-7f0e-42f8-91b0-a3f7bf5274fc.png)
The above example does not contain any sensitive information. In a real network response you will often find that the header info in the `Network` tab has a `SAMLResponse` field containing XML that has been encoded and/or encrypted. 

There are a variety of ways to decompress and decrypt XML. For an easy to use tools we recommend [samltool.com](https://www.samltool.com/), which provides a user friendly UI to accomplish these tasks.

If you're not sure why your SAML isn't working and you've collected the network request and response from your login attempts, please feel free to reach out to our support team at [support@sourcegraph.com](mailto:support@sourcegraph.com), **please redact any secret keys that may be present in your site configuration or SAML assertions before sharing with us at Sourcegraph.** 



