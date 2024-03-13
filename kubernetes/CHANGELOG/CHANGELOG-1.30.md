<!-- BEGIN MUNGE: GENERATED_TOC -->

- [v1.30.0-beta.0](#v1300-beta0)
  - [Downloads for v1.30.0-beta.0](#downloads-for-v1300-beta0)
    - [Source Code](#source-code)
    - [Client Binaries](#client-binaries)
    - [Server Binaries](#server-binaries)
    - [Node Binaries](#node-binaries)
    - [Container Images](#container-images)
  - [Changelog since v1.30.0-alpha.3](#changelog-since-v1300-alpha3)
  - [Changes by Kind](#changes-by-kind)
    - [API Change](#api-change)
    - [Feature](#feature)
    - [Bug or Regression](#bug-or-regression)
    - [Other (Cleanup or Flake)](#other-cleanup-or-flake)
  - [Dependencies](#dependencies)
    - [Added](#added)
    - [Changed](#changed)
    - [Removed](#removed)
- [v1.30.0-alpha.3](#v1300-alpha3)
  - [Downloads for v1.30.0-alpha.3](#downloads-for-v1300-alpha3)
    - [Source Code](#source-code-1)
    - [Client Binaries](#client-binaries-1)
    - [Server Binaries](#server-binaries-1)
    - [Node Binaries](#node-binaries-1)
    - [Container Images](#container-images-1)
  - [Changelog since v1.30.0-alpha.2](#changelog-since-v1300-alpha2)
  - [Changes by Kind](#changes-by-kind-1)
    - [API Change](#api-change-1)
    - [Feature](#feature-1)
    - [Documentation](#documentation)
    - [Bug or Regression](#bug-or-regression-1)
    - [Other (Cleanup or Flake)](#other-cleanup-or-flake-1)
  - [Dependencies](#dependencies-1)
    - [Added](#added-1)
    - [Changed](#changed-1)
    - [Removed](#removed-1)
- [v1.30.0-alpha.2](#v1300-alpha2)
  - [Downloads for v1.30.0-alpha.2](#downloads-for-v1300-alpha2)
    - [Source Code](#source-code-2)
    - [Client Binaries](#client-binaries-2)
    - [Server Binaries](#server-binaries-2)
    - [Node Binaries](#node-binaries-2)
    - [Container Images](#container-images-2)
  - [Changelog since v1.30.0-alpha.1](#changelog-since-v1300-alpha1)
  - [Changes by Kind](#changes-by-kind-2)
    - [Deprecation](#deprecation)
    - [API Change](#api-change-2)
    - [Feature](#feature-2)
    - [Bug or Regression](#bug-or-regression-2)
    - [Other (Cleanup or Flake)](#other-cleanup-or-flake-2)
  - [Dependencies](#dependencies-2)
    - [Added](#added-2)
    - [Changed](#changed-2)
    - [Removed](#removed-2)
- [v1.30.0-alpha.1](#v1300-alpha1)
  - [Downloads for v1.30.0-alpha.1](#downloads-for-v1300-alpha1)
    - [Source Code](#source-code-3)
    - [Client Binaries](#client-binaries-3)
    - [Server Binaries](#server-binaries-3)
    - [Node Binaries](#node-binaries-3)
    - [Container Images](#container-images-3)
  - [Changelog since v1.29.0](#changelog-since-v1290)
  - [Changes by Kind](#changes-by-kind-3)
    - [Deprecation](#deprecation-1)
    - [API Change](#api-change-3)
    - [Feature](#feature-3)
    - [Documentation](#documentation-1)
    - [Bug or Regression](#bug-or-regression-3)
    - [Other (Cleanup or Flake)](#other-cleanup-or-flake-3)
  - [Dependencies](#dependencies-3)
    - [Added](#added-3)
    - [Changed](#changed-3)
    - [Removed](#removed-3)

<!-- END MUNGE: GENERATED_TOC -->

# v1.30.0-beta.0


## Downloads for v1.30.0-beta.0



### Source Code

filename | sha512 hash
-------- | -----------
[kubernetes.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes.tar.gz) | e83f477aed051274437987d7b3fa30e923c04950c15d4a7bec20e87f54c017d5938a8d822885b0b458e31c692cade1d26567ac10ffa90934ed15890516376236
[kubernetes-src.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-src.tar.gz) | a32078a0547d093bbf7d1c323d89cbe50fa04c8d98fe9f0decf2be63d206ad11872009971fd9937336f6a7a187294b058e441297a2ae8d7620d77965ad287ecc

### Client Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-client-darwin-amd64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-darwin-amd64.tar.gz) | 948db15a9905704d08517c530f903d321103ba2c863c307d5afaa06036aa4ebca24e8674187399f9a92210e58eb7db8e0b46c7dc9f6abada19fcf64334c1ebf6
[kubernetes-client-darwin-arm64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-darwin-arm64.tar.gz) | 67312baa29835f99ca81e3f241e4f08d776ac606364b4bfbe4bdfb07b1c0a7efdb68bd2b279e07816a7779b560accf4d70e71bbae739326c19844f33c25e97f5
[kubernetes-client-linux-386.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-linux-386.tar.gz) | 0d83df79b845d22e7a0cb98a51b0f4d5e3b3c4558aea128cde5c16c0a1076096dd64569bed4485a419a755d72ba2ac27a364b0dc31319abfe1fbbc01a9b9b9eb
[kubernetes-client-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-linux-amd64.tar.gz) | 6dc7c48f7418c2375a2c0b264005aff04dca88fb6b2607b71acd5083f7ef62d907b4cdcc6353615855e675f2575fdddce0e010e994553e380ce45fd76f33a7f0
[kubernetes-client-linux-arm.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-linux-arm.tar.gz) | 98988fc90a23a5ef6e552192f44812858cb33e01378806a53853409d15927bc153b422f67563f81bb0eb0807584b08376ea76e584c5ab9faf5fab15ff73f9298
[kubernetes-client-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-linux-arm64.tar.gz) | aadab5f9253cd313a85575a1c39d4b06966826b0e76ac1b647736dadc9545b57a9a3c9663528f13fb9432e3ca4c8a59698cf445f81402d7d3fbca76f5268d2b5
[kubernetes-client-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-linux-ppc64le.tar.gz) | 710bfde17dc991a4e5a233e26ca55dcbd021e75d10d70dbdba71ad791235dbe6607322b97bd3f22eb3e4d843eefdc8f38d1f0b28fac0ce0743fb063135a136c9
[kubernetes-client-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-linux-s390x.tar.gz) | b036defee013a7187eeade78df0ab4dd221da347602cd33f977560fb89b27b82ecd7c2a9df1b63c3cef786c36ea054b735ef31fc9ad0fc4af980542a520375ef
[kubernetes-client-windows-386.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-windows-386.tar.gz) | dd4f20363812d781f9a4d7e985285418ddfd05b8ba05fd1c07c0ebbb2b3df1b940a8d57472a9b0647a6f71498be28cd8d8b71500a5576dbf7e8c3d8902b9005c
[kubernetes-client-windows-amd64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-windows-amd64.tar.gz) | 29f71f746dc3987d0187f6039b5e9c897b790c5f31882f7d3d6b138a592e384981856ced87c7cd892574566735d4c9f8972b90cd8a3370adf298f289ce32fc9d
[kubernetes-client-windows-arm64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-client-windows-arm64.tar.gz) | 805d8c10e562e45553f1a0978814924e3df5fc244868d20de77d8eea2e978ce524b4d87c5bd06a6250f087237db8566aa46edf6253e47b5b8f2651b14eb6ccdc

### Server Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-server-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-server-linux-amd64.tar.gz) | 8332ba0e47eece25af1864fe95849cabe5a208a48e5b8b4d311c545244ae1d05f0569b51f12887e97d8288ab80bc57044490153325e4af43082a65097579ded5
[kubernetes-server-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-server-linux-arm64.tar.gz) | e215b58ac54169d50e9a0247b08de1255990c77bdc80838dc226f165aacb84bd46605c3e3102a23ef590548b431a74bf9e3547fa24f3b5f84de4d68ba32965cb
[kubernetes-server-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-server-linux-ppc64le.tar.gz) | d71917d0853b448b1541b4a437a40caef3624a2dacaafb918b2f3679fbb68b94a44ac3d13bcc7b5f6adbf65913342777af39b65b31742bf5c130893d47b65f10
[kubernetes-server-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-server-linux-s390x.tar.gz) | d347add21100106c7fc057cfe0ac940fd0f80741faff9b9dc6093d3c99db17abf29b7cd713cd91f728cc1dae217ac9ad2446801f3f92c9aa18291829497aae01

### Node Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-node-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-node-linux-amd64.tar.gz) | c853ce453e49aa520e20c934849eeeca4e841d49c94bbd8951d94ebade34ed92aecc841715023e0853f23d78e9bb884d5234d790a5ffe9a9a2fa580114bd849c
[kubernetes-node-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-node-linux-arm64.tar.gz) | 91a8de520f17062f4680d7b0a7f8073cabbc0996010d4ecc0d907d0bc89bd8641bef1ace3f5d5c050ffa30ce6dec1019b80ee5acea1e3d947666a5bac826b466
[kubernetes-node-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-node-linux-ppc64le.tar.gz) | ed17879b3b43183f5a537a1bad44a56140f809f182f131dbf95b4cbd4c91d90d79016d1c6fd108025a756f408c2dee68d5c458df29b4891a7b598fa41a119a94
[kubernetes-node-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-node-linux-s390x.tar.gz) | bbbcde49cfa7dd52560865816b2c0ac92ce1e7d9a5bf17cce979adecc1b258f13cd07118e0b6c1959cca102c172ec8c950e14207d352b943d14153bb5f864555
[kubernetes-node-windows-amd64.tar.gz](https://dl.k8s.io/v1.30.0-beta.0/kubernetes-node-windows-amd64.tar.gz) | 952472d1b65a7b647d6e3f661ea36c975cf82482c32936ea2aa11ae0e828237391e7ae97d5b8a65b194178953c7725b092027ee545439a754e28702e60383e70

### Container Images

All container images are available as manifest lists and support the described
architectures. It is also possible to pull a specific architecture directly by
adding the "-$ARCH" suffix  to the container image name.

name | architectures
---- | -------------
[registry.k8s.io/conformance:v1.30.0-beta.0](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-s390x)
[registry.k8s.io/kube-apiserver:v1.30.0-beta.0](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-s390x)
[registry.k8s.io/kube-controller-manager:v1.30.0-beta.0](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-s390x)
[registry.k8s.io/kube-proxy:v1.30.0-beta.0](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-s390x)
[registry.k8s.io/kube-scheduler:v1.30.0-beta.0](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-s390x)
[registry.k8s.io/kubectl:v1.30.0-beta.0](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-s390x)

## Changelog since v1.30.0-alpha.3

## Changes by Kind

### API Change

- A new (alpha) field, `trafficDistribution`, has been added to the Service `spec`.
  This field provides a way to express preferences for how traffic is distributed to the endpoints for a Service.
  It can be enabled through the `ServiceTrafficDistribution` feature gate. ([#123487](https://github.com/kubernetes/kubernetes/pull/123487), [@gauravkghildiyal](https://github.com/gauravkghildiyal)) [SIG API Machinery, Apps and Network]
- Add alpha-level support for the SuccessPolicy in Jobs ([#123412](https://github.com/kubernetes/kubernetes/pull/123412), [@tenzen-y](https://github.com/tenzen-y)) [SIG API Machinery, Apps and Testing]
- Added (alpha) support for the managedBy field on Jobs. Jobs with a custom value of this field - any
  value other than `kubernetes.io/job-controller` - are skipped by the job controller, and their
  reconciliation is delegated to an external controller, indicated by the value of the field. Jobs that
  don't have this field at all, or where the field value is the reserved string `kubernetes.io/job-controller`,
  are reconciled by the built-in job controller. ([#123273](https://github.com/kubernetes/kubernetes/pull/123273), [@mimowo](https://github.com/mimowo)) [SIG API Machinery, Apps and Testing]
- Added a alpha feature, behind the `RelaxedEnvironmentVariableValidation` feature gate.
  When that gate is enabled, Kubernetes allows almost all printable ASCII characters to be used in the names
  of environment variables for containers in Pods. ([#123385](https://github.com/kubernetes/kubernetes/pull/123385), [@HirazawaUi](https://github.com/HirazawaUi)) [SIG Apps, Node and Testing]
- Added alpha support for field selectors on custom resources.
  Provided that the `CustomResourceFieldSelectors` feature gate is enabled, the CustomResourceDefinition
  API now lets you specify `selectableFields`. Listing a field there allows filtering custom resources for that
  CustomResourceDefinition in **list** or **watch** requests. ([#122717](https://github.com/kubernetes/kubernetes/pull/122717), [@jpbetz](https://github.com/jpbetz)) [SIG API Machinery]
- Added support for configuring multiple JWT authenticators in Structured Authentication Configuration. The maximum allowed JWT authenticators in the authentication configuration is 64. ([#123431](https://github.com/kubernetes/kubernetes/pull/123431), [@aramase](https://github.com/aramase)) [SIG Auth and Testing]
- Aggregated discovery supports both v2beta1 and v2 types and feature is promoted to GA ([#122882](https://github.com/kubernetes/kubernetes/pull/122882), [@Jefftree](https://github.com/Jefftree)) [SIG API Machinery and Testing]
- Allowing container runtimes to fix an image garbage collection bug by adding an `image_id` field to the CRI Container message. ([#123508](https://github.com/kubernetes/kubernetes/pull/123508), [@saschagrunert](https://github.com/saschagrunert)) [SIG Node]
- AppArmor profiles can now be configured through fields on the PodSecurityContext and container SecurityContext.
  - The beta AppArmor annotations are deprecated.
  - AppArmor status is no longer included in the node ready condition ([#123435](https://github.com/kubernetes/kubernetes/pull/123435), [@tallclair](https://github.com/tallclair)) [SIG API Machinery, Apps, Auth, Node and Testing]
- Conflicting issuers between JWT authenticators and service account config are now detected and fail on API server startup.  Previously such a config would run but would be inconsistently effective depending on the credential. ([#123561](https://github.com/kubernetes/kubernetes/pull/123561), [@enj](https://github.com/enj)) [SIG API Machinery and Auth]
- Dynamic Resource Allocation: DRA drivers may now use "structured parameters" to let the scheduler handle claim allocation. ([#123516](https://github.com/kubernetes/kubernetes/pull/123516), [@pohly](https://github.com/pohly)) [SIG API Machinery, Apps, Auth, CLI, Cluster Lifecycle, Instrumentation, Node, Release, Scheduling, Storage and Testing]
- Graduated pod scheduling gates to general availability.
  The `PodSchedulingReadiness` feature gate no longer has any effect, and the
  `.spec.schedulingGates` field is always available within the Pod and PodTemplate APIs. ([#123575](https://github.com/kubernetes/kubernetes/pull/123575), [@Huang-Wei](https://github.com/Huang-Wei)) [SIG API Machinery, Apps, Node, Scheduling and Testing]
- Graduated support for `minDomains` in pod topology spread constraints, to general availability.
  The `MinDomainsInPodTopologySpread` feature gate no longer has any effect, and the field is
  always available within the Pod and PodTemplate APIs. ([#123481](https://github.com/kubernetes/kubernetes/pull/123481), [@sanposhiho](https://github.com/sanposhiho)) [SIG API Machinery, Apps, Scheduling and Testing]
- JWT authenticator config set via the --authentication-config flag is now dynamically reloaded as the file changes on disk. ([#123525](https://github.com/kubernetes/kubernetes/pull/123525), [@enj](https://github.com/enj)) [SIG API Machinery, Auth and Testing]
- Kube-apiserver: the AuthenticationConfiguration type accepted in `--authentication-config` files has been promoted to `apiserver.config.k8s.io/v1beta1`. ([#123696](https://github.com/kubernetes/kubernetes/pull/123696), [@aramase](https://github.com/aramase)) [SIG API Machinery, Auth and Testing]
- Kube-apiserver: the AuthorizationConfiguration type accepted in `--authorization-config` files has been promoted to `apiserver.config.k8s.io/v1beta1`. ([#123640](https://github.com/kubernetes/kubernetes/pull/123640), [@liggitt](https://github.com/liggitt)) [SIG Auth and Testing]
- Kubelet should fail if NodeSwap is used with LimitedSwap and cgroupv1 node. ([#123738](https://github.com/kubernetes/kubernetes/pull/123738), [@kannon92](https://github.com/kannon92)) [SIG API Machinery, Node and Testing]
- Kubelet: a custom root directory for pod logs (instead of default /var/log/pods) can be specified using the `podLogsDir`
  key in kubelet configuration. ([#112957](https://github.com/kubernetes/kubernetes/pull/112957), [@mxpv](https://github.com/mxpv)) [SIG API Machinery, Node, Scalability and Testing]
- Kubelet: the `.memorySwap.swapBehavior` field in kubelet configuration accepts a new value `NoSwap` and makes this the default if unspecified; the previously accepted `UnlimitedSwap` value has been dropped. ([#122745](https://github.com/kubernetes/kubernetes/pull/122745), [@kannon92](https://github.com/kannon92)) [SIG API Machinery, Node and Testing]
- OIDC authentication will now fail if the username asserted based on a CEL expression config is the empty string.  Previously the request would be authenticated with the username set to the empty string. ([#123568](https://github.com/kubernetes/kubernetes/pull/123568), [@enj](https://github.com/enj)) [SIG API Machinery, Auth and Testing]
- PodSpec API: remove note that hostAliases are not supported on hostNetwork Pods. The feature has been supported since v1.8. ([#122422](https://github.com/kubernetes/kubernetes/pull/122422), [@neolit123](https://github.com/neolit123)) [SIG API Machinery and Apps]
- Promote AdmissionWebhookMatchConditions to GA. The feature is now stable and the feature gate is now locked to default. ([#123560](https://github.com/kubernetes/kubernetes/pull/123560), [@ivelichkovich](https://github.com/ivelichkovich)) [SIG API Machinery and Testing]
- Structured Authentication Configuration now supports `DiscoveryURL`. 
  discoveryURL if specified, overrides the URL used to fetch discovery information. 
  This is for scenarios where the well-known and jwks endpoints are hosted at a different
  location than the issuer (such as locally in the cluster). ([#123527](https://github.com/kubernetes/kubernetes/pull/123527), [@aramase](https://github.com/aramase)) [SIG API Machinery, Auth and Testing]
- Support Recursive Read-only (RRO) mounts  (KEP-3857) ([#123180](https://github.com/kubernetes/kubernetes/pull/123180), [@AkihiroSuda](https://github.com/AkihiroSuda)) [SIG API Machinery, Apps, Node and Testing]
- The StructuredAuthenticationConfiguration feature is now beta and enabled by default. ([#123719](https://github.com/kubernetes/kubernetes/pull/123719), [@enj](https://github.com/enj)) [SIG API Machinery and Auth]
- The `StorageVersionMigration` API, which was previously available as a Custom Resource Definition (CRD), is now a built-in API in Kubernetes. ([#123344](https://github.com/kubernetes/kubernetes/pull/123344), [@nilekhc](https://github.com/nilekhc)) [SIG API Machinery, Apps, Auth, CLI and Testing]
- The kubernetes repo now uses Go workspaces.  This should not impact end users at all, but does have impact for developers of downstream projects.  Switching to workspaces caused some breaking changes in the flags to the various k8s.io/code-generator tools.  Downstream consumers should look at staging/src/k8s.io/code-generator/kube_codegen.sh to see the changes. ([#123529](https://github.com/kubernetes/kubernetes/pull/123529), [@thockin](https://github.com/thockin)) [SIG API Machinery, Apps, Architecture, Auth, CLI, Cloud Provider, Cluster Lifecycle, Instrumentation, Network, Node, Release, Storage and Testing]
- ValidatingAdmissionPolicy is promoted to GA and will be enabled by default. ([#123405](https://github.com/kubernetes/kubernetes/pull/123405), [@cici37](https://github.com/cici37)) [SIG API Machinery, Apps, Auth and Testing]
- When configuring a JWT authenticator:
  
  If username.expression uses 'claims.email', then 'claims.email_verified' must be used in
  username.expression or extra[*].valueExpression or claimValidationRules[*].expression.
  An example claim validation rule expression that matches the validation automatically
  applied when username.claim is set to 'email' is 'claims.?email_verified.orValue(true)'. ([#123737](https://github.com/kubernetes/kubernetes/pull/123737), [@enj](https://github.com/enj)) [SIG API Machinery and Auth]

### Feature

- Added `access_mode` label to `volume_manager_selinux_*` metrics. ([#123667](https://github.com/kubernetes/kubernetes/pull/123667), [@jsafrane](https://github.com/jsafrane)) [SIG Node, Storage and Testing]
- Added `client-go` support for upgrading subresource fields from client-side to server-side management ([#123484](https://github.com/kubernetes/kubernetes/pull/123484), [@erikgb](https://github.com/erikgb)) [SIG API Machinery]
- Added apiserver_watch_cache_read_wait metric to measure watch cache impact on request latency. ([#123190](https://github.com/kubernetes/kubernetes/pull/123190), [@padlar](https://github.com/padlar)) [SIG API Machinery and Instrumentation]
- Adds new flag, namely `custom`, in kubectl debug to let users customize pre-defined profiles. ([#120346](https://github.com/kubernetes/kubernetes/pull/120346), [@ardaguclu](https://github.com/ardaguclu)) [SIG CLI]
- Bump cAdvisor to v0.49.0 ([#123599](https://github.com/kubernetes/kubernetes/pull/123599), [@bobbypage](https://github.com/bobbypage)) [SIG Node]
- Embed Node information into Pod-bound service account tokens as additional metadata
  - Set the 'JTI' field in issued service account tokens, and embed this information as `authentication.kubernetes.io/credential-id` in user's ExtraInfo ([#123135](https://github.com/kubernetes/kubernetes/pull/123135), [@munnerz](https://github.com/munnerz)) [SIG API Machinery, Auth and Testing]
- Feature gates for RemoteCommand (kubectl exec, cp, and attach) over WebSockets are now enabled by default (Beta).
  - Server-side feature gate: TranslateStreamCloseWebsocketRequests
  - Client-side (kubectl) feature gate: KUBECTL_REMOTE_COMMAND_WEBSOCKETS
  - To turn off RemoteCommand over WebSockets for kubectl, the environment variable feature gate must be explicitly set - KUBECTL_REMOTE_COMMAND_WEBSOCKETS=false ([#123281](https://github.com/kubernetes/kubernetes/pull/123281), [@seans3](https://github.com/seans3)) [SIG API Machinery, CLI and Testing]
- Graduated HorizontalPodAutoscaler support for per-container metrics to stable. ([#123482](https://github.com/kubernetes/kubernetes/pull/123482), [@sanposhiho](https://github.com/sanposhiho)) [SIG API Machinery, Apps and Autoscaling]
- Graduated _forensic container checkpointing_ [KEP #2008](https://kep.k8s.io/2008) from Alpha to Beta. ([#123215](https://github.com/kubernetes/kubernetes/pull/123215), [@adrianreber](https://github.com/adrianreber)) [SIG Node and Testing]
- In the Pod API, setting the alpha `procMount` field to `Unmasked` in a container now requires setting `spec.hostUsers=false` as well. ([#123520](https://github.com/kubernetes/kubernetes/pull/123520), [@haircommander](https://github.com/haircommander)) [SIG Apps, Auth and Testing]
- InitContainer's image location will be considered in scheduling when prioritizing nodes. ([#123366](https://github.com/kubernetes/kubernetes/pull/123366), [@kerthcet](https://github.com/kerthcet)) [SIG Scheduling]
- It is possible to configure the IDs that the Kubelet uses to create user namespaces.
  
  
  User namespaces support is a Beta feature now. ([#123593](https://github.com/kubernetes/kubernetes/pull/123593), [@giuseppe](https://github.com/giuseppe)) [SIG Node]
- Kube-apiserver now reports latency metric for JWT authenticator authenticate token decisions in the `apiserver_authentication_jwt_authenticator_latency_seconds` metric, labeled by jwtIssuer hash and result. ([#123225](https://github.com/kubernetes/kubernetes/pull/123225), [@aramase](https://github.com/aramase)) [SIG API Machinery and Auth]
- Kube-apiserver now reports the following metrics for authorization webhook match conditions:
  - `apiserver_authorization_match_condition_evaluation_errors_total` counter metric labeled by authorizer type and name
  - `apiserver_authorization_match_condition_exclusions_total` counter metric labeled by authorizer type and name
  - `apiserver_authorization_match_condition_evaluation_seconds` histogram metric labeled by authorizer type and name ([#123611](https://github.com/kubernetes/kubernetes/pull/123611), [@ritazh](https://github.com/ritazh)) [SIG API Machinery, Auth and Testing]
- Kube-apiserver: Authorization webhooks now report the following metrics:
  - apiserver_authorization_webhook_evaluations_total
  - apiserver_authorization_webhook_duration_seconds
  - apiserver_authorization_webhook_evaluations_fail_open_total ([#123639](https://github.com/kubernetes/kubernetes/pull/123639), [@liggitt](https://github.com/liggitt)) [SIG API Machinery, Auth and Testing]
- Kube-apiserver: JWT authenticator now report the following metrics:
  - apiserver_authentication_config_controller_automatic_reloads_total
  - apiserver_authentication_config_controller_automatic_reload_last_timestamp_seconds ([#123793](https://github.com/kubernetes/kubernetes/pull/123793), [@aramase](https://github.com/aramase)) [SIG API Machinery, Auth and Testing]
- Kube-apiserver: the StructuredAuthorizationConfiguration feature gate is promoted to beta and allows using the `--authorization-configuration` flag ([#123641](https://github.com/kubernetes/kubernetes/pull/123641), [@liggitt](https://github.com/liggitt)) [SIG API Machinery and Auth]
- Kube-controller-manager: increase the global level for broadcaster's logging to 3 so that users can ignore event messages by lowering the logging level. It reduces information noise. ([#122293](https://github.com/kubernetes/kubernetes/pull/122293), [@mengjiao-liu](https://github.com/mengjiao-liu)) [SIG API Machinery, Apps, Autoscaling, Network, Node, Scheduling, Storage and Testing]
- Kubeadm: add the WaitForAllControlPlaneComponents feature gate. It can be used to tell kubeadm to wait for all control plane components to be ready when running "kubeadm init" or "kubeadm join --control-plane". Currently kubeadm only waits for the kube-apiserver. The "kubeadm join" workflow now includes a new experimental phase called "wait-control-plane". This phase will be marked as non-experimental when WaitForAllControlPlaneComponents becomes GA. Accordingly a "kubeadm init" phase "wait-control-plane" will also be available once WaitForAllControlPlaneComponents becomes GA. These phases can be skipped if the user prefers to not wait for the control plane components. ([#123341](https://github.com/kubernetes/kubernetes/pull/123341), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: print all the kubelets and nodes that need to be upgraded on "upgrade plan". ([#123578](https://github.com/kubernetes/kubernetes/pull/123578), [@carlory](https://github.com/carlory)) [SIG Cluster Lifecycle]
- Kubectl port-forward over websockets (tunneling SPDY) can be enabled using an `Alpha` feature flag environment variable: KUBECTL_PORT_FORWARD_WEBSOCKETS=true. The API Server being communicated to must *also* have an `Alpha` feature flag enabled: PortForwardWebsockets. ([#123413](https://github.com/kubernetes/kubernetes/pull/123413), [@seans3](https://github.com/seans3)) [SIG API Machinery, CLI, Node and Testing]
- Kubernetes is now built with go 1.22.1 ([#123750](https://github.com/kubernetes/kubernetes/pull/123750), [@cpanato](https://github.com/cpanato)) [SIG Release and Testing]
- Node podresources API now includes init containers with containerRestartPolicy of `Always` when `SidecarContainers` feature is enabled. ([#120718](https://github.com/kubernetes/kubernetes/pull/120718), [@gjkim42](https://github.com/gjkim42)) [SIG Node and Testing]
- Promote ImageMaximumGCAge feature to beta ([#123424](https://github.com/kubernetes/kubernetes/pull/123424), [@haircommander](https://github.com/haircommander)) [SIG Node and Testing]
- Promote PodHostIPs condition to GA and lock to default. ([#122870](https://github.com/kubernetes/kubernetes/pull/122870), [@wzshiming](https://github.com/wzshiming)) [SIG Apps, Network, Node and Testing]
- Target drop-in kubelet configuration dir feature to Beta ([#122907](https://github.com/kubernetes/kubernetes/pull/122907), [@sohankunkerkar](https://github.com/sohankunkerkar)) [SIG Node and Testing]
- The Kubelet rejects creating the pod if hostUserns=false and the CRI runtime does not support user namespaces. ([#123216](https://github.com/kubernetes/kubernetes/pull/123216), [@giuseppe](https://github.com/giuseppe)) [SIG Node]
- The watch cache waits until it is at least as fresh as given requestedWatchRV if sendInitialEvents was requested. ([#122830](https://github.com/kubernetes/kubernetes/pull/122830), [@p0lyn0mial](https://github.com/p0lyn0mial)) [SIG API Machinery, Network and Testing]
- ValidatingAdmissionPolicy now exclude TokenReview, SelfSubjectReview, LocalSubjectAccessReview, and SubjectAccessReview from all versions of authentication.k8s.io and authorization.k8s.io group. ([#123543](https://github.com/kubernetes/kubernetes/pull/123543), [@jiahuif](https://github.com/jiahuif)) [SIG API Machinery and Testing]
- `kubectl get job` now displays the status for the listed jobs. ([#123226](https://github.com/kubernetes/kubernetes/pull/123226), [@ivanvc](https://github.com/ivanvc)) [SIG Apps and CLI]

### Bug or Regression

- Adds the namespace when using 'kubectl logs <pod-name>' and the pod is not found. Previously the message returned would be 'Error from server (NotFound): pods "my-pod-name" not found'. This has been updated to reflect the namespace in the message as follows: 'Error from server (NotFound): pods "my-pod-name" not found in namespace "default"' ([#120111](https://github.com/kubernetes/kubernetes/pull/120111), [@newtondev](https://github.com/newtondev)) [SIG CLI]
- DRA: ResourceClaim and PodSchedulingContext status updates no longer allow changing object meta data. ([#123730](https://github.com/kubernetes/kubernetes/pull/123730), [@pohly](https://github.com/pohly)) [SIG Node]
- Fix CEL estimated cost to for expressions that perform operations on the result of `map()` 
  operations, (e.g. `.map(...).exists(...)` ) to have the correct estimated instead of an unbounded 
  cost. ([#123562](https://github.com/kubernetes/kubernetes/pull/123562), [@jpbetz](https://github.com/jpbetz)) [SIG API Machinery, Auth and Cloud Provider]
- Fix node lifecycle controller panic when conditionType ready is been patch nil by mistake ([#122874](https://github.com/kubernetes/kubernetes/pull/122874), [@fusida](https://github.com/fusida)) [SIG Apps, Network and Node]
- Fix non-recursive list returning "resource version too high" error when consistent list from cache is enabled ([#123674](https://github.com/kubernetes/kubernetes/pull/123674), [@serathius](https://github.com/serathius)) [SIG API Machinery]
- Fixed a bug that an init container with containerRestartPolicy with `Always` cannot update its state from terminated to non-terminated for the pod with restartPolicy with `Never` or `OnFailure`. ([#123323](https://github.com/kubernetes/kubernetes/pull/123323), [@gjkim42](https://github.com/gjkim42)) [SIG Apps and Node]
- Fixed incorrect syncCronJob error logging. ([#122493](https://github.com/kubernetes/kubernetes/pull/122493), [@mengjiao-liu](https://github.com/mengjiao-liu)) [SIG Apps]
- Fixed the disruption controller's PDB status synchronization to maintain all PDB conditions during an update. ([#122056](https://github.com/kubernetes/kubernetes/pull/122056), [@dhenkel92](https://github.com/dhenkel92)) [SIG Apps]
- Fixes bug where providing a fieldpath to a CRD Validation Rule would erroneously affect the reported field path of other unrelated CRD Validation Rules on the same schema ([#123475](https://github.com/kubernetes/kubernetes/pull/123475), [@alexzielenski](https://github.com/alexzielenski)) [SIG API Machinery]
- JWTs used in service account and OIDC authentication are now strictly parsed to confirm that they use compact serialization.  Other encodings were not previously accepted, but would result in different unspecific errors. ([#123540](https://github.com/kubernetes/kubernetes/pull/123540), [@enj](https://github.com/enj)) [SIG API Machinery and Auth]
- Kubeadm:  in the new output API "output.kubeadm.k8s.io/v1alpha3" modify the UpgradePlan structure that is used when calling "kubeadm upgrade plan ... -o yaml|json", to include a list of multiple available upgrades. ([#123461](https://github.com/kubernetes/kubernetes/pull/123461), [@carlory](https://github.com/carlory)) [SIG Cluster Lifecycle]
- Kubeadm: avoid uploading a defaulted flag value "--authorization-mode=Node,RBAC" for the kube-apiserver in the ClusterConfiguration stored in the "kube-system/kubeadm-config" ConfigMap. "Node,RBAC" are already the kubeadm defaults for this flag, so this action is redundant. ([#123555](https://github.com/kubernetes/kubernetes/pull/123555), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- OpenAPI V2 will no longer publish aggregated apiserver OpenAPI for group-versions not matching the APIService specified group version ([#123570](https://github.com/kubernetes/kubernetes/pull/123570), [@Jefftree](https://github.com/Jefftree)) [SIG API Machinery]
- Prevent watch cache starvation by moving its watch to separate RPC and add a SeparateCacheWatchRPC feature flag to disable this behavior ([#123532](https://github.com/kubernetes/kubernetes/pull/123532), [@serathius](https://github.com/serathius)) [SIG API Machinery]
- The initialization of nodes using external cloud-providers now waits for the providerID value to be available before declaring the node ready. This is required because previously, if there were errors of communication with the cloud-provider on the cloud-controller-manager, nodes may have been declared Ready without having this field or the zone labels, and the information was never reconciled. The providerID and the zone labels are required for integrations like loadbalancers to work correctly. Users still can opt-out to this new behavior by setting the feature flag OptionalProviderID in the cloud-controller-manager. ([#123331](https://github.com/kubernetes/kubernetes/pull/123331), [@aojea](https://github.com/aojea)) [SIG API Machinery, Cloud Provider and Testing]
- The initialization of nodes using external cloud-providers now waits for the providerID value to be available before untainting it. This is required because , if there are communication errors with the cloud-provider on the cloud-controller-manager, nodes may have been declared Ready without having this field or the zone labels, and this information was never reconciled. The providerID and the zone labels are required for integrations like loadbalancers to work correctly. Cloud providers that does not implement the `GetInstanceProviderID` method will not require the providerID to be set and will not fail to initialize the node for backward compatibility issues. ([#123713](https://github.com/kubernetes/kubernetes/pull/123713), [@aojea](https://github.com/aojea)) [SIG Cloud Provider]
- Updates google.golang.org/protobuf to v1.33.0 to resolve CVE-2024-24786 ([#123758](https://github.com/kubernetes/kubernetes/pull/123758), [@liggitt](https://github.com/liggitt)) [SIG API Machinery, Architecture, Auth, CLI, Cloud Provider, Cluster Lifecycle, Instrumentation, Network, Node and Storage]
- [kubeadam][structured authz] avoid setting default `--authorization-mode` when `--authorization-config` is provided ([#123654](https://github.com/kubernetes/kubernetes/pull/123654), [@LiorLieberman](https://github.com/LiorLieberman)) [SIG Cluster Lifecycle]

### Other (Cleanup or Flake)

- Accept zero as a default value for kubectl create token duration ([#123565](https://github.com/kubernetes/kubernetes/pull/123565), [@ah8ad3](https://github.com/ah8ad3)) [SIG CLI]
- Update kubedns and nodelocaldns to v1.23.0 ([#123310](https://github.com/kubernetes/kubernetes/pull/123310), [@bzsuni](https://github.com/bzsuni)) [SIG Cloud Provider]

## Dependencies

### Added
- github.com/pkg/diff: [20ebb0f](https://github.com/pkg/diff/tree/20ebb0f)
- golang.org/x/telemetry: b75ee88
- k8s.io/gengo/v2: 51d4e06

### Changed
- github.com/docker/docker: [v20.10.24+incompatible → v20.10.27+incompatible](https://github.com/docker/docker/compare/v20.10.24...v20.10.27)
- github.com/golang/protobuf: [v1.5.3 → v1.5.4](https://github.com/golang/protobuf/compare/v1.5.3...v1.5.4)
- github.com/google/cadvisor: [v0.48.1 → v0.49.0](https://github.com/google/cadvisor/compare/v0.48.1...v0.49.0)
- github.com/google/cel-go: [v0.17.7 → v0.17.8](https://github.com/google/cel-go/compare/v0.17.7...v0.17.8)
- golang.org/x/mod: v0.14.0 → v0.15.0
- golang.org/x/net: v0.19.0 → v0.21.0
- golang.org/x/sync: v0.5.0 → v0.6.0
- golang.org/x/tools: v0.16.1 → v0.18.0
- google.golang.org/protobuf: v1.31.0 → v1.33.0
- k8s.io/kube-openapi: 778a556 → 70dd376

### Removed
- k8s.io/gengo: 9cce18d



# v1.30.0-alpha.3


## Downloads for v1.30.0-alpha.3



### Source Code

filename | sha512 hash
-------- | -----------
[kubernetes.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes.tar.gz) | adbf45f5a9c6efb135c8632e330e24e46b3ae8179372e96fbc5a016bbe089c629ee86683bdd13254a78c5f37c8576cf2364bca19961087f47c4d11a8605b7a92
[kubernetes-src.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-src.tar.gz) | d1bbeed0aca09cc6df72de4e11bd4f6869a422b947604e2a7fc32cc23f01d8a822719486f0f039ef554012e0896faf6738471412296dea069615fd48be611cda

### Client Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-client-darwin-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-darwin-amd64.tar.gz) | b1aeb5eb6480832c8ef899d7f4a7fd679d317d8704a925b426d97e49022bd4dd7bc661c530f46720d62669b0b6a0be9a94144545852108cb3062eedfd32b70d0
[kubernetes-client-darwin-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-darwin-arm64.tar.gz) | 13c34d52999172a3b73d3e4eba4029c686a8a6d3a0fa16e81d2fa1b3a9d6f7bdb37de9495fb09f783d8edfea8302e648f71d37b131826c89715baa068d555a16
[kubernetes-client-linux-386.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-linux-386.tar.gz) | 5465059af2ecf092d71d30bd5021e175590bc802c2796c366cf1eedb26fb9927f8bd637784a672242aa351a519ff807126953d6c3b940464d72bb1e46b9fbb43
[kubernetes-client-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-linux-amd64.tar.gz) | f00211e115ed1d42fc5794bbdd2f2cf9d78ab28844cf9f3b0d5abe4dcdaedca8ce66fb8045ce8688e05fd9e7b9488fcc40d9a691fc4a529cbeb7909868a092bb
[kubernetes-client-linux-arm.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-linux-arm.tar.gz) | 1bf579ba6aa68fd2ec6f539a69771c933f1df8c21f3d798d130ea6fa13a4d36919926c4212ff4f67cbc2941099720f2924ae8f5f7feab21f669cbef16a082318
[kubernetes-client-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-linux-arm64.tar.gz) | 90da779e19ccdd95673b830c9434e316d7ddd676675ce403fd4858e22e1c5afe3103a6f28c45370ff8847b62a689f279310fb390b3f9140aa77987d437ef44e2
[kubernetes-client-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-linux-ppc64le.tar.gz) | 4ecf1e5c8520c4370ad0bbf22ba3d54209224bae573836659fd0c0eca43991700bdcac609baea792a9957b497da9c20d8afa8c5152d8a2e272cca5a93a1f0e95
[kubernetes-client-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-linux-s390x.tar.gz) | 174beb0691ccfef8f0ba8fcbc2d7bda9015321b69d38e5ccd87fa0609070d8e194af435f372c76e2b65971bc2c58a053e3c5a97bca29d703305cd125e4ae7f7e
[kubernetes-client-windows-386.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-windows-386.tar.gz) | 4815aa9032e2d3d3b7a25bd1c07353ded15eda073a31b3894463e47cde0a9197324947f56f239faa671cf95caeb9c6dd377d38b4672a819f9ef781ca4b64ef18
[kubernetes-client-windows-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-windows-amd64.tar.gz) | 40fd08d6827eb182f79cefc80cc31f661aa2800e5a5cdc778f16b30a7f583ff3ee266bef04e042f598fdce34c899e5acba75ea4c5ecd84651215646bbbc15285
[kubernetes-client-windows-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-client-windows-arm64.tar.gz) | 24367addf42cc786aa3b39b51d344df65aa92fc0a4270faa9d733301ecd757d16120c70a54fd8a2d17bbff4c85ed7ff623ed2ece6e6f9a436637aba743b99aa7

### Server Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-server-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-server-linux-amd64.tar.gz) | 18bfcd3615789df2361f6acbff9a1407d5891168710264334bc60f8fbbe04dd26d88c96d02f744d2280e91dc550f0df24cd021602dcba2ae28204b1dcf723a1f
[kubernetes-server-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-server-linux-arm64.tar.gz) | 3a31aa6b074bb8ebc7fc0200a7c7821931108a572503ff2995460e28d581b3cf7beaa4407232ee22a4a52afc63f40ac549809254693289b12ddd66893f4ab2fe
[kubernetes-server-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-server-linux-ppc64le.tar.gz) | ae0602c5aa2565ef2b8afb10d28088be41c1802ed537c1d33a6a2fdba6f5c0e9ca2af8597a64a9c7244a7d2b4d75e0829eeca68f88e2de669f6a6ee7c52897ba
[kubernetes-server-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-server-linux-s390x.tar.gz) | ea3466f44bdfb250cff319f4ddf854402bc25492548b290a64b5b4e0b027dbed9e17b04ae03b2ae14cb5e30d31447d19219951dde0f2de03255ab1f6a1c3a531

### Node Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-node-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-node-linux-amd64.tar.gz) | 378c42d0640a1b845af7bb46224a19b5451452ce6ee7c4dfdb7e912f3248ec6be35c1679cc78202c548ad91b345d2ce470407db39d50dbb0cd0518a526e4429c
[kubernetes-node-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-node-linux-arm64.tar.gz) | a5ec415b0c3fbf3003f354fcf913a9851105963e5ba38c68bdebd8427eabb3f2a1598bc2688133f2ad84229218ebe18171e4a123827b9ffcb94436f69bfb43ff
[kubernetes-node-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-node-linux-ppc64le.tar.gz) | 8d9b49c3375a1dbfa24fdc46397f929b2f029a94af9cbe36387a8b22ad80f65711d10df2c5327f25cb4e4c3f91135c2f07b8726198fd8ecf1ee8aef005d2531c
[kubernetes-node-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-node-linux-s390x.tar.gz) | 99ded87a16331941cd56076cd50892446b40f09771d938552aeb9d858677bc4564472ac470273b681841c2f061836588813eb6e1065322a9ee9c72f3dfb7d58e
[kubernetes-node-windows-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.3/kubernetes-node-windows-amd64.tar.gz) | 5e9b2b95b4751c125cb3e5182ed2095829af968be3d1e9899f31febb8eaf6dd0b037e8fac48bd51a9100c1f1e90829299c117abc23e40fc66c7d709b83d1222d

### Container Images

All container images are available as manifest lists and support the described
architectures. It is also possible to pull a specific architecture directly by
adding the "-$ARCH" suffix  to the container image name.

name | architectures
---- | -------------
[registry.k8s.io/conformance:v1.30.0-alpha.3](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-s390x)
[registry.k8s.io/kube-apiserver:v1.30.0-alpha.3](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-s390x)
[registry.k8s.io/kube-controller-manager:v1.30.0-alpha.3](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-s390x)
[registry.k8s.io/kube-proxy:v1.30.0-alpha.3](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-s390x)
[registry.k8s.io/kube-scheduler:v1.30.0-alpha.3](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-s390x)
[registry.k8s.io/kubectl:v1.30.0-alpha.3](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-s390x)

## Changelog since v1.30.0-alpha.2

## Changes by Kind

### API Change

- Added a CBOR implementation of `runtime.Serializer`. Until CBOR graduates to Alpha, API servers will refuse to start if configured with CBOR support. ([#122881](https://github.com/kubernetes/kubernetes/pull/122881), [@benluddy](https://github.com/benluddy)) [SIG API Machinery]
- Added audienceMatchPolicy field to AuthenticationConfiguration and support for configuring multiple audiences.
  
  - The "audienceMatchPolicy" can be empty (or unset) when a single audience is specified in the "audiences" field.
  - The "audienceMatchPolicy" must be set to "MatchAny" when multiple audiences are specified in the "audiences" field. ([#123165](https://github.com/kubernetes/kubernetes/pull/123165), [@aramase](https://github.com/aramase)) [SIG API Machinery, Auth and Testing]
- Contextual logging is now beta and enabled by default. ([#122589](https://github.com/kubernetes/kubernetes/pull/122589), [@pohly](https://github.com/pohly)) [SIG Instrumentation]
- Cri-api: KEP-3857: Recursive Read-only (RRO) mounts ([#123272](https://github.com/kubernetes/kubernetes/pull/123272), [@AkihiroSuda](https://github.com/AkihiroSuda)) [SIG Node]
- Enabled a mechanism for concurrent log rotatation via `kubelet` using a configuration entity of `containerLogMaxWorkers` which controls the maximum number of concurrent rotation that can be performed and an interval configuration of `containerLogMonitorInterval` that can aid is configuring the monitoring duration to best suite your cluster's log generation standards. ([#114301](https://github.com/kubernetes/kubernetes/pull/114301), [@harshanarayana](https://github.com/harshanarayana)) [SIG API Machinery, Node and Testing]
- Text logging in Kubernetes components now uses [textlogger](https://pkg.go.dev/k8s.io/klog/v2@v2.120.0/textlogger). The same split streams of info and error log entries with buffering of info entries is now also supported for text output (off by default, alpha feature). Previously, this was only supported for JSON. Performance is better also without split streams. ([#114672](https://github.com/kubernetes/kubernetes/pull/114672), [@pohly](https://github.com/pohly)) [SIG API Machinery, Architecture, Auth, CLI, Cloud Provider, Cluster Lifecycle, Instrumentation, Network, Node, Storage and Testing]
- This change adds the following CLI option for `kube-controller-manager`:
  - `disable-force-detach` (defaults to `false`): Prevent force detaching volumes based on maximum unmount time and node status. If enabled, the non-graceful node shutdown feature must be used to recover from node failure (see https://kubernetes.io/blog/2023/08/16/kubernetes-1-28-non-graceful-node-shutdown-ga/). If enabled and a pod must be forcibly terminated at the risk of corruption, then the appropriate VolumeAttachment object (see here: https://kubernetes.io/docs/reference/kubernetes-api/config-and-storage-resources/volume-attachment-v1/) must be deleted. ([#120344](https://github.com/kubernetes/kubernetes/pull/120344), [@rohitssingh](https://github.com/rohitssingh)) [SIG API Machinery, Apps, Storage and Testing]

### Feature

- A new kubelet metric image_pull_duration_seconds is added. The metric tracks the duration (in seconds) it takes for an image to be pulled, including the time spent in the waiting queue of image puller. The metric is broken down by bucketed image size. ([#121719](https://github.com/kubernetes/kubernetes/pull/121719), [@ruiwen-zhao](https://github.com/ruiwen-zhao)) [SIG Instrumentation and Node]
- A new metric `lifecycle_handler_sleep_terminated_total` is added to record how many times LifecycleHandler sleep got unexpectedly terminated. ([#122456](https://github.com/kubernetes/kubernetes/pull/122456), [@AxeZhan](https://github.com/AxeZhan)) [SIG Node and Testing]
- Add "reason" field to image_garbage_collected_total metric, so admins can differentiate images that were collected for reason "age" vs "space" ([#123345](https://github.com/kubernetes/kubernetes/pull/123345), [@haircommander](https://github.com/haircommander)) [SIG Node]
- Add feature gate `MutatingAdmissionPolicy` for enabling mutation policy in admission chain. ([#123425](https://github.com/kubernetes/kubernetes/pull/123425), [@cici37](https://github.com/cici37)) [SIG API Machinery]
- Add kubelet metrics to track the memory manager  allocation and pinning ([#121778](https://github.com/kubernetes/kubernetes/pull/121778), [@Tal-or](https://github.com/Tal-or)) [SIG Node and Testing]
- Added support for cloud provider integrations to supply optional, per-Node custom labels that will be
  applied to Nodes by the node controller.
  Extra labels will only be applied where the cloud provider integration implements this. ([#123223](https://github.com/kubernetes/kubernetes/pull/123223), [@mmerkes](https://github.com/mmerkes)) [SIG Cloud Provider]
- Kube-apiserver now reloads the `--authorization-config` file when it changes. Reloads increment the `apiserver_authorization_config_controller_automatic_reload_last_timestamp_seconds` timestamp metric, with `status="success"` for successful reloads and `status="failed"` for failed reloads. Failed reloads keep using the previously loaded authorization configuration. ([#121946](https://github.com/kubernetes/kubernetes/pull/121946), [@liggitt](https://github.com/liggitt)) [SIG API Machinery, Auth and Testing]
- Kube-apiserver now reports metrics for authorization decisions in the `apiserver_authorization_decisions_total` metric, labeled by authorizer type, name, and decision. ([#123333](https://github.com/kubernetes/kubernetes/pull/123333), [@liggitt](https://github.com/liggitt)) [SIG API Machinery, Auth and Testing]
- Kubeadm: add support for machine readable output with "-o yaml" and "-o json" to the command "kubeadm certs check-expiration". This change is added in a new API "kind": "CertificateExpirationInfo",  "apiVersion": "output.kubeadm.k8s.io/v1alpha3". The existing non structured formatting is preserved. The output API version v1alpha2 is now deprecated and will be removed in a future release. Please migrate to using v1alpha3. ([#123372](https://github.com/kubernetes/kubernetes/pull/123372), [@carlory](https://github.com/carlory)) [SIG Cluster Lifecycle]
- LoadBalancerIPMode feature is now marked as Beta ([#123418](https://github.com/kubernetes/kubernetes/pull/123418), [@rikatz](https://github.com/rikatz)) [SIG Network and Testing]
- New alpha feature gate `SELinuxMount` can be used to speed up SELinux relabeling of volumes. ([#123157](https://github.com/kubernetes/kubernetes/pull/123157), [@jsafrane](https://github.com/jsafrane)) [SIG Node and Storage]
- NewVolumeManagerReconstruction feature is now GA. ([#123442](https://github.com/kubernetes/kubernetes/pull/123442), [@jsafrane](https://github.com/jsafrane)) [SIG Node]
- Promoted the `CRDValidationRatcheting` feature gate to beta, and made it enabled by default. ([#121461](https://github.com/kubernetes/kubernetes/pull/121461), [@alexzielenski](https://github.com/alexzielenski)) [SIG API Machinery and Testing]
- Update ImageGCMaxAge behavior in the kubelet to wait the MaxAge duration after the kubelet has restarted before garbage collecting ([#123343](https://github.com/kubernetes/kubernetes/pull/123343), [@haircommander](https://github.com/haircommander)) [SIG Node and Testing]
- When the RetryGenerateName feature gate is enabled on the kube-apiserver,
  create requests using generateName are retried automatically by the apiserver when the generated name conflicts with an existing resource name, up to a max limit of 7 retries.
  This feature is in alpha. ([#122887](https://github.com/kubernetes/kubernetes/pull/122887), [@jpbetz](https://github.com/jpbetz)) [SIG API Machinery]

### Documentation

- Add a new internal metric in the kubelet that allow developers to understand the source of the latency problems on node startups.
  
  kubelet_first_network_pod_start_sli_duration_seconds ([#121720](https://github.com/kubernetes/kubernetes/pull/121720), [@aojea](https://github.com/aojea)) [SIG Instrumentation, Network and Node]

### Bug or Regression

- DRA: fixed potential data race with no known real-world implications. ([#123222](https://github.com/kubernetes/kubernetes/pull/123222), [@pohly](https://github.com/pohly)) [SIG Node]
- Fix bug where health check could pass while APIServices are missing from aggregated discovery ([#122883](https://github.com/kubernetes/kubernetes/pull/122883), [@Jefftree](https://github.com/Jefftree)) [SIG API Machinery and Testing]
- Fixed an issue where a JWT authenticator configured via --authentication-config would fail to verify tokens that were not signed using RS256. ([#123282](https://github.com/kubernetes/kubernetes/pull/123282), [@enj](https://github.com/enj)) [SIG API Machinery, Auth and Testing]
- Improves scheduler performance when no scoring plugins are defined. ([#123384](https://github.com/kubernetes/kubernetes/pull/123384), [@aleksandra-malinowska](https://github.com/aleksandra-malinowska)) [SIG Scheduling]
- Kubeadm: fix a bug during kubeadm upgrade, where it is not possible to mount a new device and create a symbolic link for /etc/kubernetes (or a sub-directory) so that kubeadm stores its information on the mounted device. ([#123406](https://github.com/kubernetes/kubernetes/pull/123406), [@SataQiu](https://github.com/SataQiu)) [SIG Cluster Lifecycle]
- Kubeadm: fix a bug where "kubeadm upgrade plan -o yaml|json" includes unneeded output and was missing component config information. ([#123492](https://github.com/kubernetes/kubernetes/pull/123492), [@carlory](https://github.com/carlory)) [SIG Cluster Lifecycle]
- Patches a leak of a discovery document that would occur when an Aggregated APIService changed its Spec.Service field and did not change it back. ([#123517](https://github.com/kubernetes/kubernetes/pull/123517), [@Jefftree](https://github.com/Jefftree)) [SIG API Machinery]
- Restore --verify-only function in code generation wrappers. ([#123261](https://github.com/kubernetes/kubernetes/pull/123261), [@skitt](https://github.com/skitt)) [SIG API Machinery]
- Sample-apiserver manifest example will have correct RBAC ([#123479](https://github.com/kubernetes/kubernetes/pull/123479), [@Jefftree](https://github.com/Jefftree)) [SIG API Machinery and Testing]

### Other (Cleanup or Flake)

- An optimization is implemented to reduce stack memory usage for watch requests.  It is can be disabled with the feature gate: APIServingWithRoutine=false ([#120902](https://github.com/kubernetes/kubernetes/pull/120902), [@linxiulei](https://github.com/linxiulei)) [SIG API Machinery]
- Kubeadm: make sure that a variety of API server requests are retried during "init", "join", "upgrade", "reset" workflows. Prior to this change some API server requests, such as, creating or updating ConfigMaps were "one-shot" - i.e. they could fail if the API server dropped connectivity for a very short period of time. ([#123271](https://github.com/kubernetes/kubernetes/pull/123271), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: the bridge-nf-call-iptables=1 and bridge-nf-call-ip6tables=1 preflight checks are removed since not all the network implementations require this setting, network plugins are responsible for setting this correctly depending on whether or not they connect containers to Linux bridges or use some other mechanism. ([#123464](https://github.com/kubernetes/kubernetes/pull/123464), [@SataQiu](https://github.com/SataQiu)) [SIG Cluster Lifecycle]
- Upgrade metrics server to v0.7.0 ([#123504](https://github.com/kubernetes/kubernetes/pull/123504), [@pacoxu](https://github.com/pacoxu)) [SIG Cloud Provider and Instrumentation]

## Dependencies

### Added
_Nothing has changed._

### Changed
- github.com/fxamacker/cbor/v2: [v2.5.0 → v2.6.0](https://github.com/fxamacker/cbor/compare/v2.5.0...v2.6.0)
- golang.org/x/crypto: v0.16.0 → v0.19.0
- golang.org/x/sys: v0.15.0 → v0.17.0
- golang.org/x/term: v0.15.0 → v0.17.0

### Removed
_Nothing has changed._



# v1.30.0-alpha.2


## Downloads for v1.30.0-alpha.2



### Source Code

filename | sha512 hash
-------- | -----------
[kubernetes.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes.tar.gz) | b6946e906e2d089431132ff4d8e24cb1b61f676f4df09b21b22a472c5aa796513ce8d7c39a312c8c0447ba0bb6cb5c4157c2be7645f91d6cf949a03a01cf9458
[kubernetes-src.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-src.tar.gz) | a339603f532774a24d9dcbde8ebc2188729a469cc670ba5f00a09cf8465f2e00bb364b5f6739d79dfac9d20a7347f495672d2f184cfce73407925e0314633a3b

### Client Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-client-darwin-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-darwin-amd64.tar.gz) | 2930b28b275662ac7a78e6d59539809138b173a930c360a417f429bbcf31e7c3ef0a1a544028c5f81e1972a9f07ac0b459f6c02e97d7c0ccbcaa39ed229ef60a
[kubernetes-client-darwin-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-darwin-arm64.tar.gz) | 6e8131d70116dce503a6800504ac349c9e4f3d359c31821083ceab936b8bd782a5f2e3027b4222fa133b7d27def3b15312fa022eb421ce2b3cfdd89f75300b5b
[kubernetes-client-linux-386.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-linux-386.tar.gz) | 9272c915586ab46cd9cef8b7029958e7c9771a0109f83eb0d9991bfe7c0468a5c6d55329e656be9cf13217b6a06875bdde2eec1a870328397a54500836267ab8
[kubernetes-client-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-linux-amd64.tar.gz) | fd8d6c83b91b13b80dd2a3000ae11746e664039fcf4bd7f1704dc6e53391e0114ab9d53dee83edb29d54ddd22d6ec042735b1e6e0930626f441147e6f4b4cfe7
[kubernetes-client-linux-arm.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-linux-arm.tar.gz) | 57b1df4ea4fedd6555dd297808ac23e9ffd7da4b5fd4876088863a287edef34b0d697f296c3da405649146c4c84f72e41155dcf858990ae6e810adb800452539
[kubernetes-client-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-linux-arm64.tar.gz) | 83e61c039bd2a7d113b68c97a06e55deff2633abd9e6f1afa98ef22a4308383f2fba3309e3b9ba23f27d0d6a3a99232e0b3404f3848c94f927d654e6317f300a
[kubernetes-client-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-linux-ppc64le.tar.gz) | cf78c218e4c23e1ad13dc75b465d38c57c2fc284eafe342adaf3b84568965f3629e2c5543c38f2c24e93ca8f5ef72c755c401fd9b5f46e8742095734784f324a
[kubernetes-client-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-linux-s390x.tar.gz) | 64913790635f51dc012d463b4f2483453483d21c6d228f2c2ac740b8c1abcf25251baffca8331c7d34a8eb945df96efd24f4d23089cc13c992baddb678ebe2b3
[kubernetes-client-windows-386.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-windows-386.tar.gz) | 066fe65b02c68858f09119b657d23b19d770f1432790666e80fd2644251cfc949d323857d5e2308a865442714138be40ee7269e8109314d3e9e99e7917380786
[kubernetes-client-windows-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-windows-amd64.tar.gz) | 057b9d0eac9d6f8f96b29a237692f346bab054947d6493fa1b75d143d457c146e46713694e5987e5fc7adf2950d5a16a974f1eb6ffb204a992b6d852435910b6
[kubernetes-client-windows-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-client-windows-arm64.tar.gz) | 0338179407fca68fc67e019fa89075eef497a130d7a09f974692b715a803e1d6521d8d31d55421117e6cefc5aee2902b3afc095fdcacd06438a1673ba9a23cd6

### Server Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-server-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-server-linux-amd64.tar.gz) | fb41f7e577b6e2501819cbb71761e29e38d50d0279fa41508af63ea3857c0c05ca5feb584f65d784d1fb6f765d6c7e9d479c91f904feebd297b05ef296567ce8
[kubernetes-server-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-server-linux-arm64.tar.gz) | 273796e1bcea82151b64974f000813f9e8e63bf8314dc2980d99610363967a8928e52d4958a03f413cb762d69b3d89918e43dac33921f2855acace09d5a74e47
[kubernetes-server-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-server-linux-ppc64le.tar.gz) | 14061e55d204a09e0c1ac7c55931ee62ca1ce9e4c843bd4c7ad42c746a5ab6812d74642bf16146d6191dc72432ebb1fc1304e9486643adfcc8419c46753b4d74
[kubernetes-server-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-server-linux-s390x.tar.gz) | d1a4ef0c30d68eda1710c032ded345acfc295a33aff37b01cb185bc5643efb1a9c27ac90dfb5afa4f95741b03ff4a55a11063e06b720715f425e9178da9ed3f9

### Node Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-node-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-node-linux-amd64.tar.gz) | 6c9589d2dc82cc838ef27f2370d503f2750aa8feaef592dd7353bd74a482a2904078df3a3488ccd3e6f64f180f1d27b8931b75f7cc97f4a1f9d543299f0b8db8
[kubernetes-node-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-node-linux-arm64.tar.gz) | 862d0c46d911ce78d191b0996e74263fc14db461cacfb8fb4fdddf4b6b982f4f72feaa1cba960c30dc0af007e718f2266a18e87cdda87fca54c511ab667773da
[kubernetes-node-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-node-linux-ppc64le.tar.gz) | 35bcf7be699b443f69b76b7133e94da69c234e3d4d021a3e41a0f09837466521d032422eaf6fd7dbc9b96eccdc97ec5c3a339bd410d1befcd1cad2de1efbd7f6
[kubernetes-node-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-node-linux-s390x.tar.gz) | 15a52713d9640ca4365a9ba40b3523e658a2889bd1e25b3e40d97d78bc03ce3d2e189d9696210059438393a4decc636e164d92d716d0c7eadd35ff7c22bcd3b3
[kubernetes-node-windows-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.2/kubernetes-node-windows-amd64.tar.gz) | 0452a35597a22014571bac052947cc751d3ac78ac02cc6b9cee206e12717930f847cde3fe84d7f44c52b274c00513c2d7c4423b1d69ee50c25973371803e49cb

### Container Images

All container images are available as manifest lists and support the described
architectures. It is also possible to pull a specific architecture directly by
adding the "-$ARCH" suffix  to the container image name.

name | architectures
---- | -------------
[registry.k8s.io/conformance:v1.30.0-alpha.2](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/conformance) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/conformance-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/conformance-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/conformance-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/conformance-s390x)
[registry.k8s.io/kube-apiserver:v1.30.0-alpha.2](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-apiserver) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-apiserver-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-apiserver-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-apiserver-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-apiserver-s390x)
[registry.k8s.io/kube-controller-manager:v1.30.0-alpha.2](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-controller-manager) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-controller-manager-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-controller-manager-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-controller-manager-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-controller-manager-s390x)
[registry.k8s.io/kube-proxy:v1.30.0-alpha.2](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-proxy) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-proxy-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-proxy-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-proxy-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-proxy-s390x)
[registry.k8s.io/kube-scheduler:v1.30.0-alpha.2](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-scheduler) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-scheduler-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-scheduler-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-scheduler-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-scheduler-s390x)
[registry.k8s.io/kubectl:v1.30.0-alpha.2](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kubectl) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kubectl-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kubectl-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kubectl-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kubectl-s390x)

## Changelog since v1.30.0-alpha.1

## Changes by Kind

### Deprecation

- Removed the `SecurityContextDeny` admission plugin, deprecated since v1.27. The Pod Security Admission plugin, available since v1.25, is recommended instead. See https://kubernetes.io/docs/reference/access-authn-authz/admission-controllers/#securitycontextdeny for more information. ([#122612](https://github.com/kubernetes/kubernetes/pull/122612), [@mtardy](https://github.com/mtardy)) [SIG Auth, Security and Testing]

### API Change

- Updated an audit annotation key used by the `…/serviceaccounts/<name>/token` resource handler.
  The annotation used to persist the issued credential identifier is now `authentication.kubernetes.io/issued-credential-id`. ([#123098](https://github.com/kubernetes/kubernetes/pull/123098), [@munnerz](https://github.com/munnerz)) [SIG Auth]

### Feature

- Add apiserver.latency.k8s.io/decode-response-object annotation to the audit log to record the decoding time ([#121512](https://github.com/kubernetes/kubernetes/pull/121512), [@HirazawaUi](https://github.com/HirazawaUi)) [SIG API Machinery]
- Added apiserver_encryption_config_controller_automatic_reloads_total to measure total number of reload successes and failures of encryption configuration. This metric contains the `status` label with enum value of `success` and `failure`.
    - Deprecated apiserver_encryption_config_controller_automatic_reload_success_total and apiserver_encryption_config_controller_automatic_reload_failure_total metrics. Use apiserver_encryption_config_controller_automatic_reloads_total instead. ([#123179](https://github.com/kubernetes/kubernetes/pull/123179), [@aramase](https://github.com/aramase)) [SIG API Machinery, Auth and Testing]
- Allow a zero value for the 'nominalConcurrencyShares' field of the PriorityLevelConfiguration object 
  either using the flowcontrol.apiserver.k8s.io/v1 or flowcontrol.apiserver.k8s.io/v1beta3 API ([#123001](https://github.com/kubernetes/kubernetes/pull/123001), [@tkashem](https://github.com/tkashem)) [SIG API Machinery]
- Graduated support for passing dual-stack `kubelet --node-ip` values when using
  a cloud provider. The feature is now GA and the `CloudDualStackNodeIPs` feature
  gate is always enabled. ([#123134](https://github.com/kubernetes/kubernetes/pull/123134), [@danwinship](https://github.com/danwinship)) [SIG API Machinery, Cloud Provider and Node]
- Kubernetes is now built with go 1.22 ([#123217](https://github.com/kubernetes/kubernetes/pull/123217), [@cpanato](https://github.com/cpanato)) [SIG Release and Testing]
- The scheduler retries Pods, which are failed by nodevolumelimits due to not found PVCs, only when new PVCs are added. ([#121952](https://github.com/kubernetes/kubernetes/pull/121952), [@sanposhiho](https://github.com/sanposhiho)) [SIG Scheduling and Storage]
- Update distroless-iptables to v0.5.0 debian-base to bookworm-v1.0.1 and setcap to bookworm-v1.0.1 ([#123170](https://github.com/kubernetes/kubernetes/pull/123170), [@cpanato](https://github.com/cpanato)) [SIG API Machinery, Architecture, Cloud Provider, Release, Storage and Testing]
- Users can traverse all the pods that are in the scheduler and waiting in the permit stage through method `IterateOverWaitingPods`. In other words,  all waitingPods in scheduler can be obtained from any profiles. Before this commit, each profile could only obtain waitingPods within that profile. ([#122946](https://github.com/kubernetes/kubernetes/pull/122946), [@NoicFank](https://github.com/NoicFank)) [SIG Scheduling]
- ValidatingAdmissionPolicy now supports type checking policies that make use of `variables`. ([#123083](https://github.com/kubernetes/kubernetes/pull/123083), [@jiahuif](https://github.com/jiahuif)) [SIG API Machinery]

### Bug or Regression

- Fix Pod stuck in Terminating because of GenerateUnmapVolumeFunc missing globalUnmapPath when kubelet tries to clean up all volumes that failed reconstruction. ([#123032](https://github.com/kubernetes/kubernetes/pull/123032), [@carlory](https://github.com/carlory)) [SIG Storage]
- Fix deprecated version for pod_scheduling_duration_seconds that caused the metric to be hidden by default in 1.29. ([#123038](https://github.com/kubernetes/kubernetes/pull/123038), [@alculquicondor](https://github.com/alculquicondor)) [SIG Instrumentation and Scheduling]
- Fix error when trying to expand a volume that does not require node expansion ([#123055](https://github.com/kubernetes/kubernetes/pull/123055), [@gnufied](https://github.com/gnufied)) [SIG Node and Storage]
- Fix the following volume plugins may not create user visible files after kubelet was restarted. 
  - configmap 
  - secret 
  - projected
  - downwardapi ([#122807](https://github.com/kubernetes/kubernetes/pull/122807), [@carlory](https://github.com/carlory)) [SIG Storage]
- Fixed cleanup of Pod volume mounts when a file was used as a subpath. ([#123052](https://github.com/kubernetes/kubernetes/pull/123052), [@jsafrane](https://github.com/jsafrane)) [SIG Node]
- Fixes an issue calculating total CPU usage reported for Windows nodes ([#122999](https://github.com/kubernetes/kubernetes/pull/122999), [@marosset](https://github.com/marosset)) [SIG Node and Windows]
- Fixing issue where AvailableBytes sometimes does not report correctly on WindowsNodes when PodAndContainerStatsFromCRI feature is enabled. ([#122846](https://github.com/kubernetes/kubernetes/pull/122846), [@marosset](https://github.com/marosset)) [SIG Node and Windows]
- Kubeadm: do not upload kubelet patch configuration into `kube-system/kubelet-config` ConfigMap ([#123093](https://github.com/kubernetes/kubernetes/pull/123093), [@SataQiu](https://github.com/SataQiu)) [SIG Cluster Lifecycle]
- Kubeadm: fix a bug where the --rootfs global flag does not work with "kubeadm upgrade node" for control plane nodes. ([#123077](https://github.com/kubernetes/kubernetes/pull/123077), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: kubelet-finalize phase of "kubeadm init" no longer requires kubelet kubeconfig to have a specific authinfo ([#123171](https://github.com/kubernetes/kubernetes/pull/123171), [@vrutkovs](https://github.com/vrutkovs)) [SIG Cluster Lifecycle]
- Show enum values in kubectl explain if they were defined ([#123023](https://github.com/kubernetes/kubernetes/pull/123023), [@ah8ad3](https://github.com/ah8ad3)) [SIG CLI]

### Other (Cleanup or Flake)

- Build etcd image v3.5.12 ([#123069](https://github.com/kubernetes/kubernetes/pull/123069), [@bzsuni](https://github.com/bzsuni)) [SIG API Machinery and Etcd]
- Fix registered wildcard clusterEvents doesn't work in scheduler requeueing. ([#123117](https://github.com/kubernetes/kubernetes/pull/123117), [@kerthcet](https://github.com/kerthcet)) [SIG Scheduling]
- Promote feature-gate LegacyServiceAccountTokenCleanUp to GA and lock to default ([#122635](https://github.com/kubernetes/kubernetes/pull/122635), [@carlory](https://github.com/carlory)) [SIG API Machinery, Auth and Testing]
- Update etcd to version 3.5.12 ([#123150](https://github.com/kubernetes/kubernetes/pull/123150), [@bzsuni](https://github.com/bzsuni)) [SIG API Machinery, Cloud Provider, Cluster Lifecycle and Testing]

## Dependencies

### Added
- github.com/fxamacker/cbor/v2: [v2.5.0](https://github.com/fxamacker/cbor/v2/tree/v2.5.0)
- github.com/x448/float16: [v0.8.4](https://github.com/x448/float16/tree/v0.8.4)

### Changed
- github.com/opencontainers/runc: [v1.1.11 → v1.1.12](https://github.com/opencontainers/runc/compare/v1.1.11...v1.1.12)
- sigs.k8s.io/apiserver-network-proxy/konnectivity-client: v0.28.0 → v0.29.0

### Removed
_Nothing has changed._



# v1.30.0-alpha.1


## Downloads for v1.30.0-alpha.1



### Source Code

filename | sha512 hash
-------- | -----------
[kubernetes.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes.tar.gz) | f9e74c1f8400e8c85a65cf85418a95e06a558d230539f4b2f7882b96709eeb3656277a7a1e59ccd699a085d6c94d31bd2dcc83a48669d610ca2064a0c978cbeb
[kubernetes-src.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-src.tar.gz) | 413f02b4cba6db36625a14095fb155b12685991ae4ece29e9d91016714aadcfbd06ac88f7766a0943445d05145980a54208cc2ed9bc29f3976f0b61a1492ace2

### Client Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-client-darwin-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-darwin-amd64.tar.gz) | d06d723da34e021db3dba1890970f5dc5e27209befb4da9cc5a8255bd124e1ea31c273d71c0ee864166acb2afa0cb08a492896c3e85efeccbbb02685c1a3b271
[kubernetes-client-darwin-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-darwin-arm64.tar.gz) | 7132d1a1ad0f6222eae02251ecd9f6df5dfbf26c6f7f789d1e81d756049eccdd68fc3f6710606bce12b24b887443553198efc801be55e94d83767341f306650e
[kubernetes-client-linux-386.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-linux-386.tar.gz) | 09500370309fe1d6472535ed048a5f173ef3bd3e12cbc74ba67e48767b07e7b295df78cabffa5eda140e659da602d17b961563a2ef2a20b2d38074d826a47a35
[kubernetes-client-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-linux-amd64.tar.gz) | 154dafa5fae88a8aeed82c0460fa37679da60327fdab8f966357fbcb905e6e6b5473eacb524c39adddccf245fcf3dea8d5715a497f0230d98df21c4cb3b450eb
[kubernetes-client-linux-arm.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-linux-arm.tar.gz) | d055b29111a90b2c19e9f45bd56e2ba0b779dc35562f21330cda7ed57d945a65343552019f0efe159a87e3a2973c9f0b86f8c16edebdb44b8b8f773354fec7b3
[kubernetes-client-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-linux-arm64.tar.gz) | c498a0c7b4ce59b198105c88ef1d29a8c345f3e1b31ba083c3f79bfcca35ae32776fd38a3b6b0bad187e14f7d54eeb0e2471634caac631039a989bd6119ab244
[kubernetes-client-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-linux-ppc64le.tar.gz) | 50e5c8bb07fac4304b067a161c34021d0c090bb5d04aed2eff4d43cab5a8cdcffc72fe97b4231f986a5b55987ebc6f6142a7e779b82ad49a109d772c3eade979
[kubernetes-client-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-linux-s390x.tar.gz) | 91b10c0f531ba530ca9766e509d1bb717531ff70061735082664da8a2bd7b3282743f53a60d74a5cb1867206f06287aa60fdec1bb41c77b14748330c5ce1199c
[kubernetes-client-windows-386.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-windows-386.tar.gz) | eaa83eab240ccf54ad54e0f66eba55bd4b15c7c37ea9a015b2b69638d90a1d5e146f989912c7745e0cbb52f846aa0135dd943b2b4b600fcbc3f9c43352f678f3
[kubernetes-client-windows-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-windows-amd64.tar.gz) | 874ad471bc887f0ae2c73d636475793716021b688baf9ae85bd9229d9ceb5ec4bab3bc9f423e2665b2a6f33697d0f5c0a838f274bb4539ea0031018687f39e85
[kubernetes-client-windows-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-client-windows-arm64.tar.gz) | 5f20a1efba7eec42f1ff1811af3b7c2703d7323e5577fd131fe79c8e53da33973a7922e794f4bc64f1fa16696cdc01e4826d0878a2e46158350a9b6de4eb345b

### Server Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-server-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-server-linux-amd64.tar.gz) | fd631b9f8e500eee418a680bd5ee104508192136701642938167f8b42ee4d2577092bada924e7b56d05db534920faeca416292bf0c1636f816ac35db30d80693
[kubernetes-server-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-server-linux-arm64.tar.gz) | cc20574eac935a61e9c23c056d8c325cf095e4217d7d23d278dcf0d2ca32c2651febd3eb3de51536fd48e0fd17cf6ec156bdcf53178c1959efc92e078d9aed44
[kubernetes-server-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-server-linux-ppc64le.tar.gz) | e8aa36ba41856b7e73fe4a52e725b1b52c70701822f17af10b3ddd03566cf41ab280b69a99c39b8dca85a0b7d80c3f88f7b0b5d5cd1da551701958f8bd176a11
[kubernetes-server-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-server-linux-s390x.tar.gz) | fdf61522374eeccda5c32b6c9dc5927a92f68c78af811976f798dce483856ebc1e52a6a2b08a121ba7a3b60f0f8e2d727814ff7aed7edd1e7282288a1cacb742

### Node Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-node-linux-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-node-linux-amd64.tar.gz) | cc8d03394114c292eca5be257b667d5114d7934f58d1c14365ea0a68fdb4e699437f3ea1a28476c65a1247cf5b877e40c0dabd295792d2d0de160f2807f9a7de
[kubernetes-node-linux-arm64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-node-linux-arm64.tar.gz) | 1602ecf70f2d9e8ec077bdb4d45a18027c702be24d474c3fdaf6ad2e3a56527ee533b53a1b4bbbe501404cc3f2d7d60a88f7f083352a57944e20b4d7109109e6
[kubernetes-node-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-node-linux-ppc64le.tar.gz) | 6494efec3efb3b0cc20170948eb2eb2e1a51c4913d26c0682de4ddcb4c20629232bc83020f62c1c618986df598008047258019e31d0ec444308064fafdbc861c
[kubernetes-node-linux-s390x.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-node-linux-s390x.tar.gz) | 265041c73c045f567e6d014b594910524daef10cc0ce27ad760fb0188c34aeee52588dc1fbef1d9f474d11d032946bdbd527e9c04196294991d0fbe71ae5e678
[kubernetes-node-windows-amd64.tar.gz](https://dl.k8s.io/v1.30.0-alpha.1/kubernetes-node-windows-amd64.tar.gz) | faa5b4598326a9bd08715f5d6d0c1ac2f47fb20c0eb5745352f76b779d99a20480a9a79c6549e352d2a092b829e1926990b5fa859392603c1c510bf571b6094f

### Container Images

All container images are available as manifest lists and support the described
architectures. It is also possible to pull a specific architecture directly by
adding the "-$ARCH" suffix  to the container image name.

name | architectures
---- | -------------
[registry.k8s.io/conformance:v1.30.0-alpha.1](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/conformance) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/conformance-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/conformance-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/conformance-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/conformance-s390x)
[registry.k8s.io/kube-apiserver:v1.30.0-alpha.1](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-apiserver) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-apiserver-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-apiserver-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-apiserver-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-apiserver-s390x)
[registry.k8s.io/kube-controller-manager:v1.30.0-alpha.1](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-controller-manager) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-controller-manager-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-controller-manager-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-controller-manager-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-controller-manager-s390x)
[registry.k8s.io/kube-proxy:v1.30.0-alpha.1](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-proxy) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-proxy-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-proxy-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-proxy-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-proxy-s390x)
[registry.k8s.io/kube-scheduler:v1.30.0-alpha.1](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-scheduler) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-scheduler-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-scheduler-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-scheduler-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kube-scheduler-s390x)
[registry.k8s.io/kubectl:v1.30.0-alpha.1](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kubectl) | [amd64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kubectl-amd64), [arm64](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kubectl-arm64), [ppc64le](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kubectl-ppc64le), [s390x](https://console.cloud.google.com/gcr/images/k8s-artifacts-prod/us/kubectl-s390x)

## Changelog since v1.29.0

## Changes by Kind

### Deprecation

- Kubectl: remove deprecated flag prune-whitelist for apply, use flag prune-allowlist instead. ([#120246](https://github.com/kubernetes/kubernetes/pull/120246), [@pacoxu](https://github.com/pacoxu)) [SIG CLI and Testing]

### API Change

- Add CEL library for IP Addresses and CIDRs. This will not be available for use until 1.31. ([#121912](https://github.com/kubernetes/kubernetes/pull/121912), [@JoelSpeed](https://github.com/JoelSpeed)) [SIG API Machinery]
- Added to MutableFeatureGate the ability to override the default setting of feature gates, to allow default-enabling a feature on a component-by-component basis instead of for all affected components simultaneously. ([#122647](https://github.com/kubernetes/kubernetes/pull/122647), [@benluddy](https://github.com/benluddy)) [SIG API Machinery and Cluster Lifecycle]
- Adds a rule on the kube_codegen tool to ignore vendor folder during the code generation. ([#122729](https://github.com/kubernetes/kubernetes/pull/122729), [@jparrill](https://github.com/jparrill)) [SIG API Machinery and Cluster Lifecycle]
- Allow users to mutate FSGroupPolicy and PodInfoOnMount in CSIDriver.Spec ([#116209](https://github.com/kubernetes/kubernetes/pull/116209), [@haoruan](https://github.com/haoruan)) [SIG API Machinery, Storage and Testing]
- Client-go events: `NewEventBroadcasterAdapterWithContext` should be used instead of `NewEventBroadcasterAdapter` if the goal is to support contextual logging. ([#122142](https://github.com/kubernetes/kubernetes/pull/122142), [@pohly](https://github.com/pohly)) [SIG API Machinery, Instrumentation and Scheduling]
- Fixes accidental enablement of the new alpha `optionalOldSelf` API field in CustomResourceDefinition validation rules, which should only be allowed to be set when the CRDValidationRatcheting feature gate is enabled. ([#122329](https://github.com/kubernetes/kubernetes/pull/122329), [@jpbetz](https://github.com/jpbetz)) [SIG API Machinery]
- Implement  `prescore` extension point for `volumeBinding` plugin. Return skip if it doesn't do anything in Score. ([#115768](https://github.com/kubernetes/kubernetes/pull/115768), [@AxeZhan](https://github.com/AxeZhan)) [SIG Scheduling, Storage and Testing]
- Resource.k8s.io/ResourceClaim (alpha API): the strategic merge patch strategy for the `status.reservedFor` array was changed such that a strategic-merge-patch can add individual entries. This breaks clients using strategic merge patch to update status which rely on the previous behavior (replacing the entire array). ([#122276](https://github.com/kubernetes/kubernetes/pull/122276), [@pohly](https://github.com/pohly)) [SIG API Machinery]
- When scheduling a mixture of pods using ResourceClaims and others which don't, scheduling a pod with ResourceClaims impacts scheduling latency less. ([#121876](https://github.com/kubernetes/kubernetes/pull/121876), [@pohly](https://github.com/pohly)) [SIG API Machinery, Node, Scheduling and Testing]

### Feature

- Add Timezone column in the output of kubectl get cronjob command ([#122231](https://github.com/kubernetes/kubernetes/pull/122231), [@ardaguclu](https://github.com/ardaguclu)) [SIG CLI]
- Add `WatchListClient` feature gate to `client-go`. When enabled it allows the client to get a stream of individual items instead of chunking from the server. ([#122571](https://github.com/kubernetes/kubernetes/pull/122571), [@p0lyn0mial](https://github.com/p0lyn0mial)) [SIG API Machinery]
- Add process_start_time_seconds to /metrics/slis endpoint of all components ([#122750](https://github.com/kubernetes/kubernetes/pull/122750), [@Richabanker](https://github.com/Richabanker)) [SIG Architecture, Instrumentation and Testing]
- Adds exec-interactive-mode and exec-provide-cluster-info flags in kubectl config set-credentials command ([#122023](https://github.com/kubernetes/kubernetes/pull/122023), [@ardaguclu](https://github.com/ardaguclu)) [SIG CLI]
- Allow scheduling framework plugins that implement io.Closer to be gracefully closed. ([#122498](https://github.com/kubernetes/kubernetes/pull/122498), [@Gekko0114](https://github.com/Gekko0114)) [SIG Scheduling]
- Change --nodeport-addresses behavior to default to "primary node IP(s) only" rather than "all node IPs". ([#122724](https://github.com/kubernetes/kubernetes/pull/122724), [@nayihz](https://github.com/nayihz)) [SIG Network and Windows]
- Etcd: build image for v3.5.11 ([#122233](https://github.com/kubernetes/kubernetes/pull/122233), [@mzaian](https://github.com/mzaian)) [SIG API Machinery]
- Informers now support adding Indexers after the informer starts ([#117046](https://github.com/kubernetes/kubernetes/pull/117046), [@howardjohn](https://github.com/howardjohn)) [SIG API Machinery]
- Introduce a feature gate mechanism to client-go. Depending on the actual implementation, users can control features via environmental variables or command line options. ([#122555](https://github.com/kubernetes/kubernetes/pull/122555), [@p0lyn0mial](https://github.com/p0lyn0mial)) [SIG API Machinery]
- Kube-scheduler implements scheduling hints for the NodeAffinity plugin.
  The scheduling hints allow the scheduler to only retry scheduling a Pod
  that was previously rejected by the NodeAffinity plugin if a new Node or a Node update matches the Pod's node affinity. ([#122309](https://github.com/kubernetes/kubernetes/pull/122309), [@carlory](https://github.com/carlory)) [SIG Scheduling]
- Kube-scheduler implements scheduling hints for the NodeResourceFit plugin.
  The scheduling hints allow the scheduler to only retry scheduling a Pod
  that was previously rejected by the NodeResourceFit plugin if a new Node or 
  a Node update matches the Pod's resource requirements or if an old pod update 
  or delete matches the  Pod's resource requirements. ([#119177](https://github.com/kubernetes/kubernetes/pull/119177), [@carlory](https://github.com/carlory)) [SIG Scheduling]
- Kube-scheduler implements scheduling hints for the NodeUnschedulable plugin.
  The scheduling hints allow the scheduler to only retry scheduling a Pod
  that was previously rejected by the NodeSchedulable plugin if a new Node or a Node update sets .spec.unschedulable to false. ([#122334](https://github.com/kubernetes/kubernetes/pull/122334), [@carlory](https://github.com/carlory)) [SIG Scheduling]
- Kube-scheduler implements scheduling hints for the PodTopologySpread plugin.
  The scheduling hints allow the scheduler to retry scheduling a Pod
  that was previously rejected by the PodTopologySpread plugin if create/delete/update a related Pod or a node which matches the toplogyKey. ([#122195](https://github.com/kubernetes/kubernetes/pull/122195), [@nayihz](https://github.com/nayihz)) [SIG Scheduling]
- Kubeadm: add better handling of errors during unmount when calling "kubeadm reset". When failing to unmount directories under "/var/run/kubelet", kubeadm will now throw an error instead of showing a warning and continuing to cleanup said directory. In such situations it is better for you to inspect the problem and resolve it manually, then you can call "kubeadm reset" again to complete the cleanup. ([#122530](https://github.com/kubernetes/kubernetes/pull/122530), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubectl debug: add sysadmin profile ([#119200](https://github.com/kubernetes/kubernetes/pull/119200), [@eiffel-fl](https://github.com/eiffel-fl)) [SIG CLI and Testing]
- Kubernetes is now built with Go 1.21.6 ([#122705](https://github.com/kubernetes/kubernetes/pull/122705), [@cpanato](https://github.com/cpanato)) [SIG Architecture, Release and Testing]
- Kubernetes is now built with go 1.22rc2 ([#122889](https://github.com/kubernetes/kubernetes/pull/122889), [@cpanato](https://github.com/cpanato)) [SIG Release and Testing]
- Print more information when kubectl describe a VolumeAttributesClass ([#122640](https://github.com/kubernetes/kubernetes/pull/122640), [@carlory](https://github.com/carlory)) [SIG CLI]
- Promote KubeProxyDrainingTerminatingNodes to Beta ([#122914](https://github.com/kubernetes/kubernetes/pull/122914), [@alexanderConstantinescu](https://github.com/alexanderConstantinescu)) [SIG Network]
- Promote feature gate StableLoadBalancerNodeSet to GA ([#122961](https://github.com/kubernetes/kubernetes/pull/122961), [@alexanderConstantinescu](https://github.com/alexanderConstantinescu)) [SIG API Machinery, Cloud Provider and Network]
- Scheduler skips NodeAffinity Score plugin when NodeAffinity Score plugin has nothing to do with a Pod.
  You might notice an increase in the metric plugin_execution_duration_seconds for extension_point=score plugin=NodeAffinity, because the plugin will only run when the plugin is relevant ([#117024](https://github.com/kubernetes/kubernetes/pull/117024), [@sanposhiho](https://github.com/sanposhiho)) [SIG Scheduling and Testing]
- The option `ignorable` of scheduler extender can skip error both filter and bind. ([#122503](https://github.com/kubernetes/kubernetes/pull/122503), [@sunbinnnnn](https://github.com/sunbinnnnn)) [SIG Scheduling]
- Update kubedns and nodelocaldns to release version 1.22.28 ([#121908](https://github.com/kubernetes/kubernetes/pull/121908), [@mzaian](https://github.com/mzaian)) [SIG Cloud Provider]
- Update some interfaces' signature in scheduler:
  
  1. PluginsRunner: use NodeInfo in `RunPreScorePlugins` and `RunScorePlugins`.
  2. PreScorePlugin: use NodeInfo in `PreScore`.
  3. Extender: use NodeInfo in `Filter` and `Prioritize`. ([#121954](https://github.com/kubernetes/kubernetes/pull/121954), [@AxeZhan](https://github.com/AxeZhan)) [SIG Autoscaling, Node, Scheduling, Storage and Testing]
- When PreFilterResult filters out some Nodes, the scheduling framework assumes them as rejected via `UnschedulableAndUnresolvable`, 
  that is those nodes won't be in the candidates of preemption process.
  Also, corrected how the scheduling framework handle Unschedulable status from PreFilter. 
  Before this PR, if PreFilter return `Unschedulable`, it may result in an unexpected abortion in the preemption, 
  which shouldn't happen in the default scheduler, but may happen in schedulers with a custom plugin. ([#119779](https://github.com/kubernetes/kubernetes/pull/119779), [@sanposhiho](https://github.com/sanposhiho)) [SIG Scheduling]
- `kubectl describe`: added Suspend to job, and Node-Selectors and Tolerations to pod template output ([#122618](https://github.com/kubernetes/kubernetes/pull/122618), [@ivanvc](https://github.com/ivanvc)) [SIG CLI]

### Documentation

- A deprecated flag `--pod-max-in-unschedulable-pods-duration` was initially planned to be removed in v1.26, but we have to change this plan. We found [an issue](https://github.com/kubernetes/kubernetes/issues/110175) in which Pods can be stuck in the unschedulable pod pool for 5 min, and using this flag is the only workaround for this issue. 
  This issue only could happen if you use custom plugins or if you change plugin set being used in your scheduler via the scheduler config. ([#122013](https://github.com/kubernetes/kubernetes/pull/122013), [@sanposhiho](https://github.com/sanposhiho)) [SIG Scheduling]
- Fix delete pod declare no controllor note. ([#120159](https://github.com/kubernetes/kubernetes/pull/120159), [@Ithrael](https://github.com/Ithrael)) [SIG CLI]

### Bug or Regression

- Add imagefs.inodesfree to default EvictionHard settings ([#121834](https://github.com/kubernetes/kubernetes/pull/121834), [@vaibhav2107](https://github.com/vaibhav2107)) [SIG Node]
- Added metric name along with the utilization information when running kubectl get hpa ([#122804](https://github.com/kubernetes/kubernetes/pull/122804), [@sreeram-venkitesh](https://github.com/sreeram-venkitesh)) [SIG CLI]
- Allow deletion of pods that use raw block volumes on node reboot ([#122211](https://github.com/kubernetes/kubernetes/pull/122211), [@gnufied](https://github.com/gnufied)) [SIG Node and Storage]
- Changed the API server so that for admission webhooks that have a URL matching the hostname `localhost`, or a loopback IP address, the connection supports HTTP/2 where it can be negotiated. ([#122558](https://github.com/kubernetes/kubernetes/pull/122558), [@linxiulei](https://github.com/linxiulei)) [SIG API Machinery and Testing]
- Etcd: Update to v3.5.11 ([#122393](https://github.com/kubernetes/kubernetes/pull/122393), [@mzaian](https://github.com/mzaian)) [SIG API Machinery, Cloud Provider, Cluster Lifecycle, Etcd and Testing]
- Fix Windows credential provider cannot find binary. Windows credential provider binary path may have ".exe" suffix so it is better to use LookPath() to support it flexibly. ([#120291](https://github.com/kubernetes/kubernetes/pull/120291), [@lzhecheng](https://github.com/lzhecheng)) [SIG Cloud Provider]
- Fix an issue where kubectl apply could panic when imported as a library ([#122346](https://github.com/kubernetes/kubernetes/pull/122346), [@Jefftree](https://github.com/Jefftree)) [SIG CLI]
- Fix panic of Evented PLEG during kubelet start-up ([#122475](https://github.com/kubernetes/kubernetes/pull/122475), [@pacoxu](https://github.com/pacoxu)) [SIG Node]
- Fix resource deletion failure caused by quota calculation error when InPlacePodVerticalScaling is turned on ([#122701](https://github.com/kubernetes/kubernetes/pull/122701), [@carlory](https://github.com/carlory)) [SIG API Machinery, Node and Testing]
- Fix the following volume plugins may not create user visible files after kubelet was restarted. 
  - configmap 
  - secret 
  - projected
  - downwardapi ([#122807](https://github.com/kubernetes/kubernetes/pull/122807), [@carlory](https://github.com/carlory)) [SIG Storage]
- Fix: Ignore unnecessary node events and improve daemonset controller performance. ([#121669](https://github.com/kubernetes/kubernetes/pull/121669), [@xigang](https://github.com/xigang)) [SIG Apps]
- Fix: Mount point may become local without calling NodePublishVolume after node rebooting. ([#119923](https://github.com/kubernetes/kubernetes/pull/119923), [@cvvz](https://github.com/cvvz)) [SIG Node and Storage]
- Fixed a bug where kubectl drain would consider a pod as having been deleted if an error occurs while calling the API. ([#122574](https://github.com/kubernetes/kubernetes/pull/122574), [@brianpursley](https://github.com/brianpursley)) [SIG CLI]
- Fixed a regression since 1.24 in the scheduling framework when overriding MultiPoint plugins (e.g. default plugins).
  The incorrect loop logic might lead to a plugin being loaded multiple times, consequently preventing any Pod from being scheduled, which is unexpected. ([#122068](https://github.com/kubernetes/kubernetes/pull/122068), [@caohe](https://github.com/caohe)) [SIG Scheduling]
- Fixed migration of in-tree vSphere volumes to the CSI driver. ([#122341](https://github.com/kubernetes/kubernetes/pull/122341), [@jsafrane](https://github.com/jsafrane)) [SIG Storage]
- Fixes a race condition in the iptables mode of kube-proxy in 1.27 and later
  that could result in some updates getting lost (e.g., when a service gets a
  new endpoint, the rules for the new endpoint might not be added until
  much later). ([#122204](https://github.com/kubernetes/kubernetes/pull/122204), [@danwinship](https://github.com/danwinship)) [SIG Network]
- Fixes bug in ValidatingAdmissionPolicy which caused policies using CRD params to not successfully sync ([#123003](https://github.com/kubernetes/kubernetes/pull/123003), [@alexzielenski](https://github.com/alexzielenski)) [SIG API Machinery and Testing]
- For statically provisioned PVs, if its volume source is CSI type or it has migrated annotation, when it's deleted, the PersisentVolume controller won't changes its phase to the Failed state. 
  
  With this patch, the external provisioner can remove the finalizer in next reconcile loop. Unfortunately if the provious existing pv has the Failed state, this patch won't take effort. It requires users to remove finalizer. ([#122030](https://github.com/kubernetes/kubernetes/pull/122030), [@carlory](https://github.com/carlory)) [SIG Apps and Storage]
- If a pvc has an empty storageClassName, persistentvolume controller won't try to assign a default StorageClass ([#122704](https://github.com/kubernetes/kubernetes/pull/122704), [@carlory](https://github.com/carlory)) [SIG Apps and Storage]
- Improves scheduler performance when no scoring plugins are defined. ([#122058](https://github.com/kubernetes/kubernetes/pull/122058), [@aleksandra-malinowska](https://github.com/aleksandra-malinowska)) [SIG Scheduling]
- Improves scheduler performance when no scoring plugins are defined. ([#122435](https://github.com/kubernetes/kubernetes/pull/122435), [@aleksandra-malinowska](https://github.com/aleksandra-malinowska)) [SIG Scheduling]
- Kube-proxy: fixed LoadBalancerSourceRanges not working for nftables mode ([#122614](https://github.com/kubernetes/kubernetes/pull/122614), [@tnqn](https://github.com/tnqn)) [SIG Network]
- Kubeadm: fix a regression in "kubeadm init" that caused a user-specified --kubeconfig file to be ignored. ([#122735](https://github.com/kubernetes/kubernetes/pull/122735), [@avorima](https://github.com/avorima)) [SIG Cluster Lifecycle]
- Make decoding etcd's response respect the timeout context. ([#121815](https://github.com/kubernetes/kubernetes/pull/121815), [@HirazawaUi](https://github.com/HirazawaUi)) [SIG API Machinery]
- QueueingHint implementation for NodeAffinity is reverted because we found potential scenarios where events that make Pods schedulable could be missed. ([#122285](https://github.com/kubernetes/kubernetes/pull/122285), [@sanposhiho](https://github.com/sanposhiho)) [SIG Scheduling]
- QueueingHint implementation for NodeUnschedulable is reverted because we found potential scenarios where events that make Pods schedulable could be missed. ([#122288](https://github.com/kubernetes/kubernetes/pull/122288), [@sanposhiho](https://github.com/sanposhiho)) [SIG Scheduling]
- Remove wrong warning event (FileSystemResizeFailed) during a pod creation if it uses a readonly volume and the capacity of the volume is greater or equal to its request storage. ([#122508](https://github.com/kubernetes/kubernetes/pull/122508), [@carlory](https://github.com/carlory)) [SIG Storage]
- Reverts the EventedPLEG feature (beta, but disabled by default) back to alpha for a known issue ([#122697](https://github.com/kubernetes/kubernetes/pull/122697), [@pacoxu](https://github.com/pacoxu)) [SIG Node]
- The scheduling queue didn't notice any extenders' failures, it could miss some cluster events,
  and it could end up Pods rejected by Extenders stuck in unschedulable pod pool in 5min in the worst-case scenario.
  Now, the scheduling queue notices extenders' failures and requeue Pods rejected by Extenders appropriately. ([#122022](https://github.com/kubernetes/kubernetes/pull/122022), [@sanposhiho](https://github.com/sanposhiho)) [SIG Scheduling]
- Use errors.Is() to handle err returned by LookPath() ([#122600](https://github.com/kubernetes/kubernetes/pull/122600), [@lzhecheng](https://github.com/lzhecheng)) [SIG Cloud Provider]
- ValidateVolumeAttributesClassUpdate also validates new vac object. ([#122449](https://github.com/kubernetes/kubernetes/pull/122449), [@carlory](https://github.com/carlory)) [SIG Storage]
- When using a claim with immediate allocation and a pod referencing that claim couldn't get scheduled, the scheduler incorrectly may have tried to deallocate that claim. ([#122415](https://github.com/kubernetes/kubernetes/pull/122415), [@pohly](https://github.com/pohly)) [SIG Node and Scheduling]

### Other (Cleanup or Flake)

- Add warning for PV on relaim policy when it is Recycle ([#122339](https://github.com/kubernetes/kubernetes/pull/122339), [@carlory](https://github.com/carlory)) [SIG Storage]
- Cleanup: remove getStorageAccountName warning messages ([#121983](https://github.com/kubernetes/kubernetes/pull/121983), [@andyzhangx](https://github.com/andyzhangx)) [SIG Cloud Provider and Storage]
- Client-go: Optimized leaders renewing leases by updating leader lock optimistically without getting the record from the apiserver first. Also added a new metric `leader_election_slowpath_total` that allow users to monitor how many leader elections are updated non-optimistically. ([#122069](https://github.com/kubernetes/kubernetes/pull/122069), [@linxiulei](https://github.com/linxiulei)) [SIG API Machinery, Architecture and Instrumentation]
- Kube-proxy nftables mode is now compatible with kernel 5.4 ([#122296](https://github.com/kubernetes/kubernetes/pull/122296), [@tnqn](https://github.com/tnqn)) [SIG Network]
- Kubeadm: improve the overall logic, error handling and output messages when waiting for the kubelet and API server /healthz endpoints to return 'ok'. The kubelet and API server checks no longer run in parallel, but one after another (in serial). ([#121958](https://github.com/kubernetes/kubernetes/pull/121958), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: show the supported shell types of 'kubeadm completion' in the error message when an invalid shell was specified ([#122477](https://github.com/kubernetes/kubernetes/pull/122477), [@SataQiu](https://github.com/SataQiu)) [SIG Cluster Lifecycle]
- Kubeadm: use `ttlSecondsAfterFinished` to automatically clean up the `upgrade-health-check` Job that runs during upgrade preflighting. ([#122079](https://github.com/kubernetes/kubernetes/pull/122079), [@carlory](https://github.com/carlory)) [SIG Cluster Lifecycle]
- Lock GA feature-gate ConsistentHTTPGetHandlers to default ([#122578](https://github.com/kubernetes/kubernetes/pull/122578), [@carlory](https://github.com/carlory)) [SIG Node]
- Migrate client-go/metadata to contextual logging ([#122225](https://github.com/kubernetes/kubernetes/pull/122225), [@ricardoapl](https://github.com/ricardoapl)) [SIG API Machinery]
- Migrated the cmd/kube-proxy to use [contextual logging](https://k8s.io/docs/concepts/cluster-administration/system-logs/#contextual-logging). ([#122197](https://github.com/kubernetes/kubernetes/pull/122197), [@fatsheep9146](https://github.com/fatsheep9146)) [SIG Network]
- Remove GA featuregate RemoveSelfLink ([#122468](https://github.com/kubernetes/kubernetes/pull/122468), [@carlory](https://github.com/carlory)) [SIG API Machinery]
- Remove GA featuregate about ExperimentalHostUserNamespaceDefaultingGate in 1.30 ([#122088](https://github.com/kubernetes/kubernetes/pull/122088), [@bzsuni](https://github.com/bzsuni)) [SIG Node]
- Remove GA featuregate about IPTablesOwnershipCleanup in 1.30 ([#122137](https://github.com/kubernetes/kubernetes/pull/122137), [@bzsuni](https://github.com/bzsuni)) [SIG Network]
- Removed generally available feature gate `ExpandedDNSConfig`. ([#122086](https://github.com/kubernetes/kubernetes/pull/122086), [@bzsuni](https://github.com/bzsuni)) [SIG Network]
- Removed generally available feature gate `KubeletPodResourcesGetAllocatable`. ([#122138](https://github.com/kubernetes/kubernetes/pull/122138), [@ii2day](https://github.com/ii2day)) [SIG Node]
- Removed generally available feature gate `KubeletPodResources`. ([#122139](https://github.com/kubernetes/kubernetes/pull/122139), [@bzsuni](https://github.com/bzsuni)) [SIG Node]
- Removed generally available feature gate `MinimizeIPTablesRestore`. ([#122136](https://github.com/kubernetes/kubernetes/pull/122136), [@ty-dc](https://github.com/ty-dc)) [SIG Network]
- Removed generally available feature gate `ProxyTerminatingEndpoints`. ([#122134](https://github.com/kubernetes/kubernetes/pull/122134), [@ty-dc](https://github.com/ty-dc)) [SIG Network]
- Removed the deprecated `azureFile` in-tree storage plugin ([#122576](https://github.com/kubernetes/kubernetes/pull/122576), [@carlory](https://github.com/carlory)) [SIG API Machinery, Cloud Provider, Node and Storage]
- Setting `--cidr-allocator-type` to `CloudAllocator` for `kube-controller-manager` will be removed in a future release. Please switch to and explore the options available in your external cloud provider ([#123011](https://github.com/kubernetes/kubernetes/pull/123011), [@dims](https://github.com/dims)) [SIG API Machinery and Network]
- The GA feature-gate APISelfSubjectReview is removed, and the feature is unconditionally enabled. ([#122032](https://github.com/kubernetes/kubernetes/pull/122032), [@carlory](https://github.com/carlory)) [SIG Auth and Testing]
- The feature gate `LegacyServiceAccountTokenTracking` (GA since 1.28) is now removed, since the feature is unconditionally enabled. ([#122409](https://github.com/kubernetes/kubernetes/pull/122409), [@Rei1010](https://github.com/Rei1010)) [SIG Auth]
- The in-tree cloud provider for azure has now been removed. Please use the external cloud provider and CSI driver from https://github.com/kubernetes/cloud-provider-azure instead. ([#122857](https://github.com/kubernetes/kubernetes/pull/122857), [@nilo19](https://github.com/nilo19)) [SIG API Machinery, Cloud Provider, Instrumentation, Node and Testing]
- The in-tree cloud provider for vSphere has now been removed. Please use the external cloud provider and CSI driver from https://github.com/kubernetes/cloud-provider-vsphere instead. ([#122937](https://github.com/kubernetes/kubernetes/pull/122937), [@dims](https://github.com/dims)) [SIG API Machinery, Cloud Provider, Storage and Testing]
- Update kube-dns to v1.22.27 ([#121736](https://github.com/kubernetes/kubernetes/pull/121736), [@ty-dc](https://github.com/ty-dc)) [SIG Cloud Provider]
- Updated cni-plugins to v1.4.0. ([#122178](https://github.com/kubernetes/kubernetes/pull/122178), [@saschagrunert](https://github.com/saschagrunert)) [SIG Cloud Provider, Node and Testing]
- Updated cri-tools to v1.29.0. ([#122271](https://github.com/kubernetes/kubernetes/pull/122271), [@saschagrunert](https://github.com/saschagrunert)) [SIG Cloud Provider]

## Dependencies

### Added
- sigs.k8s.io/knftables: v0.0.14

### Changed
- github.com/go-logr/logr: [v1.3.0 → v1.4.1](https://github.com/go-logr/logr/compare/v1.3.0...v1.4.1)
- github.com/go-logr/zapr: [v1.2.3 → v1.3.0](https://github.com/go-logr/zapr/compare/v1.2.3...v1.3.0)
- github.com/onsi/ginkgo/v2: [v2.13.0 → v2.15.0](https://github.com/onsi/ginkgo/v2/compare/v2.13.0...v2.15.0)
- github.com/onsi/gomega: [v1.29.0 → v1.31.0](https://github.com/onsi/gomega/compare/v1.29.0...v1.31.0)
- github.com/opencontainers/runc: [v1.1.10 → v1.1.11](https://github.com/opencontainers/runc/compare/v1.1.10...v1.1.11)
- go.uber.org/atomic: v1.10.0 → v1.7.0
- go.uber.org/goleak: v1.2.1 → v1.3.0
- go.uber.org/zap: v1.19.0 → v1.26.0
- golang.org/x/crypto: v0.14.0 → v0.16.0
- golang.org/x/mod: v0.12.0 → v0.14.0
- golang.org/x/net: v0.17.0 → v0.19.0
- golang.org/x/sync: v0.3.0 → v0.5.0
- golang.org/x/sys: v0.13.0 → v0.15.0
- golang.org/x/term: v0.13.0 → v0.15.0
- golang.org/x/text: v0.13.0 → v0.14.0
- golang.org/x/tools: v0.12.0 → v0.16.1
- k8s.io/klog/v2: v2.110.1 → v2.120.1
- k8s.io/kube-openapi: 2dd684a → 778a556

### Removed
- github.com/Azure/azure-sdk-for-go: [v68.0.0+incompatible](https://github.com/Azure/azure-sdk-for-go/tree/v68.0.0)
- github.com/Azure/go-autorest/autorest/adal: [v0.9.23](https://github.com/Azure/go-autorest/autorest/adal/tree/v0.9.23)
- github.com/Azure/go-autorest/autorest/date: [v0.3.0](https://github.com/Azure/go-autorest/autorest/date/tree/v0.3.0)
- github.com/Azure/go-autorest/autorest/mocks: [v0.4.2](https://github.com/Azure/go-autorest/autorest/mocks/tree/v0.4.2)
- github.com/Azure/go-autorest/autorest/to: [v0.4.0](https://github.com/Azure/go-autorest/autorest/to/tree/v0.4.0)
- github.com/Azure/go-autorest/autorest/validation: [v0.3.1](https://github.com/Azure/go-autorest/autorest/validation/tree/v0.3.1)
- github.com/Azure/go-autorest/autorest: [v0.11.29](https://github.com/Azure/go-autorest/autorest/tree/v0.11.29)
- github.com/Azure/go-autorest/logger: [v0.2.1](https://github.com/Azure/go-autorest/logger/tree/v0.2.1)
- github.com/Azure/go-autorest/tracing: [v0.6.0](https://github.com/Azure/go-autorest/tracing/tree/v0.6.0)
- github.com/Azure/go-autorest: [v14.2.0+incompatible](https://github.com/Azure/go-autorest/tree/v14.2.0)
- github.com/a8m/tree: [10a5fd5](https://github.com/a8m/tree/tree/10a5fd5)
- github.com/benbjohnson/clock: [v1.1.0](https://github.com/benbjohnson/clock/tree/v1.1.0)
- github.com/danwinship/knftables: [v0.0.13](https://github.com/danwinship/knftables/tree/v0.0.13)
- github.com/dnaeon/go-vcr: [v1.2.0](https://github.com/dnaeon/go-vcr/tree/v1.2.0)
- github.com/dougm/pretty: [2ee9d74](https://github.com/dougm/pretty/tree/2ee9d74)
- github.com/gofrs/uuid: [v4.4.0+incompatible](https://github.com/gofrs/uuid/tree/v4.4.0)
- github.com/rasky/go-xdr: [4930550](https://github.com/rasky/go-xdr/tree/4930550)
- github.com/rubiojr/go-vhd: [02e2102](https://github.com/rubiojr/go-vhd/tree/02e2102)
- github.com/vmware/govmomi: [v0.30.6](https://github.com/vmware/govmomi/tree/v0.30.6)
- github.com/vmware/vmw-guestinfo: [25eff15](https://github.com/vmware/vmw-guestinfo/tree/25eff15)