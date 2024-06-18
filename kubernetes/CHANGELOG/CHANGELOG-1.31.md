<!-- BEGIN MUNGE: GENERATED_TOC -->

- [v1.31.0-alpha.2](#v1310-alpha2)
  - [Downloads for v1.31.0-alpha.2](#downloads-for-v1310-alpha2)
    - [Source Code](#source-code)
    - [Client Binaries](#client-binaries)
    - [Server Binaries](#server-binaries)
    - [Node Binaries](#node-binaries)
    - [Container Images](#container-images)
  - [Changelog since v1.31.0-alpha.1](#changelog-since-v1310-alpha1)
  - [Urgent Upgrade Notes](#urgent-upgrade-notes)
    - [(No, really, you MUST read this before you upgrade)](#no-really-you-must-read-this-before-you-upgrade)
  - [Changes by Kind](#changes-by-kind)
    - [API Change](#api-change)
    - [Feature](#feature)
    - [Failing Test](#failing-test)
    - [Bug or Regression](#bug-or-regression)
    - [Other (Cleanup or Flake)](#other-cleanup-or-flake)
  - [Dependencies](#dependencies)
    - [Added](#added)
    - [Changed](#changed)
    - [Removed](#removed)
- [v1.31.0-alpha.1](#v1310-alpha1)
  - [Downloads for v1.31.0-alpha.1](#downloads-for-v1310-alpha1)
    - [Source Code](#source-code-1)
    - [Client Binaries](#client-binaries-1)
    - [Server Binaries](#server-binaries-1)
    - [Node Binaries](#node-binaries-1)
    - [Container Images](#container-images-1)
  - [Changelog since v1.30.0](#changelog-since-v1300)
  - [Urgent Upgrade Notes](#urgent-upgrade-notes-1)
    - [(No, really, you MUST read this before you upgrade)](#no-really-you-must-read-this-before-you-upgrade-1)
  - [Changes by Kind](#changes-by-kind-1)
    - [Deprecation](#deprecation)
    - [API Change](#api-change-1)
    - [Feature](#feature-1)
    - [Failing Test](#failing-test-1)
    - [Bug or Regression](#bug-or-regression-1)
    - [Other (Cleanup or Flake)](#other-cleanup-or-flake-1)
  - [Dependencies](#dependencies-1)
    - [Added](#added-1)
    - [Changed](#changed-1)
    - [Removed](#removed-1)

<!-- END MUNGE: GENERATED_TOC -->

# v1.31.0-alpha.2


## Downloads for v1.31.0-alpha.2



### Source Code

filename | sha512 hash
-------- | -----------
[kubernetes.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes.tar.gz) | 16c79d46cc58352ebccbe1be1139dfc8cfd6ac522fa6e08ea54dbf1d5f9544a508431f43f82670f1a554ac9a7059307a74e20c1927f41c013cacad80951bf47d
[kubernetes-src.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-src.tar.gz) | ad7637808305ea59cd61e27788fb81f51b0e5c41355c189f689a7a8e58e0d1b6fb0cd278a29fa0c74b6307b1af3a37667650e72bb6d1796b2dc1c7c13f3f4539

### Client Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-client-darwin-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-darwin-amd64.tar.gz) | 358ee8f7e6a3afa76bdc96a2c11463b42421ee5d41ec6f3eeaaf86ccd34eb433b0c0b20bf0097085758aa95b63dce18357d34f885662724c1d965ff4f2bd21a2
[kubernetes-client-darwin-arm64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-darwin-arm64.tar.gz) | 2ce564a16b49f4da3e2fa322c3c1ee4fcc02b9a12f8261232d835094222455c9b2c55dd5fce7980aa5bf87e40752875b2124e31e93db9558ca25a4a466beec15
[kubernetes-client-linux-386.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-linux-386.tar.gz) | fcd1e9ed89366af00091d3626d4e3513d3ea329b25e0a4b701f981d384bc71f2a348ccd99e6c329e7076cd75dab9dc13ab31b4818b24596996711bc034c58400
[kubernetes-client-linux-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-linux-amd64.tar.gz) | e7b705bb04de6eca9a633a4ff3c2486e486cbd61c77a7c75c6d94f1b5612ed1e6f852c060c0194d5c2bfd84d905cdb8ea3b19ddbedb46e458b23db82c547d3a7
[kubernetes-client-linux-arm.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-linux-arm.tar.gz) | 93c5385082ecf84757ff6830678f5469a5b2463687d8a256f920c0fd25ed4c08bd06ec2beaf507d0bbe10d9489632349ee552d8d3f8f861c9977ff307bb89f23
[kubernetes-client-linux-arm64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-linux-arm64.tar.gz) | e9427fe6e034e9cec5b522b01797e3144f08ba60a01cd0c86eba7cb27811e470c0e3eb007e6432f4d9005a2cc57253956b66cd2eb68cd4ae73659193733910df
[kubernetes-client-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-linux-ppc64le.tar.gz) | 91a3b101a9c5f513291bf80452d3023c0000078c16720a2874dd554c23a87d15632f9e1bf419614e0e3a9d8b2f3f177eee3ef08d405aca3c7e3311dec3dfebba
[kubernetes-client-linux-s390x.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-linux-s390x.tar.gz) | 1cda8de38ccdc8fbf2a0c74bd5d35b4638f6c40c5aa157e2ade542225462a662e4415f3d3abb31c1d1783c7267f16530b3b392e72b75b9d5f797f7137eecba66
[kubernetes-client-windows-386.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-windows-386.tar.gz) | c7fe8d85f00150778cc3f3bde20a627cd160f495a7dcd2cf67beb1604c29b2f06c9e521d7c3249a89595d8cda4c2f6ac652fa27ec0dd761e1f2539edcbb5d0ef
[kubernetes-client-windows-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-windows-amd64.tar.gz) | 62315ae783685e5cb74e149da7d752973d115e95d5c0e58c1c06f8ceec925f4310fb9c220be42bad6fd7dc4ff0540343a4fff12767a5eb305a29ff079f3b940a
[kubernetes-client-windows-arm64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-client-windows-arm64.tar.gz) | eddd92de174554f026d77f333eac5266621cffe0d07ad5c32cf26d46f831742fa3b6f049494eb1a4143e90fdded80a795e5ddce37be5f15cd656bdc102f3fcb2

### Server Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-server-linux-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-server-linux-amd64.tar.gz) | c561ebdfb17826faefcc44d4b9528890a9141a31e6d1a6935cce88a4265ba10eddbd0726bd32cffcdd09374247a1d5faf911ca717afc9669716c6a2e61741e65
[kubernetes-server-linux-arm64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-server-linux-arm64.tar.gz) | 3faed373e59cef714034110cdbdd33d861b72e939058a193f230908fea4961550216490e5eca43ffaa838cd9c2884267c685a0f4e2fc686fd0902bbb2d97a01c
[kubernetes-server-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-server-linux-ppc64le.tar.gz) | 336d38807433e32cdcb09a0a2ee8cbb7eb2d13c9d991c5fc228298c0bec13d45b4b001db96199498a2f0f106d27c716963b6c49b9f40e07f8800801e3cea5ec9
[kubernetes-server-linux-s390x.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-server-linux-s390x.tar.gz) | 2ebc780b3323db8209f017262e3a01c040d3ee986bdd0732085fbe945cb0e135a1c8bd4adf31ded6576e19e2b5370efded9f149ef724ad5e2bbddf981e8c1bda

### Node Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-node-linux-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-node-linux-amd64.tar.gz) | 2e6d0a1d6698be1ffeadf54da70cb4b988ead6ed9e232372d008f2ec49cb1dd9e30efa5a2cc7f1768d1b9c6facde002b39931433e3a239df46f6db0c067dbbac
[kubernetes-node-linux-arm64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-node-linux-arm64.tar.gz) | 7fa93c164097f60bb6dcbaccf87024a5c6fb300915a46bf1824c57472d198c6e52c39fa27d0e3cd55acb55833579dd6ddb4024e1500f7998140ef10dbec47b22
[kubernetes-node-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-node-linux-ppc64le.tar.gz) | b6ae26348b3703680c15d508b65253d0e58d93d3b435668a40a1d5dd65b5ed6ab2b0190ca6ea77d2091f7223dad225e3f671ae72bda4ed5be0d29b753ad498b6
[kubernetes-node-linux-s390x.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-node-linux-s390x.tar.gz) | 5247cfa0499518bc5f00dda154c1dd36ef5b62e1a2861deb3a36e3a5651eefd05f7a2004eba6500912cafd81ce485f172014c8680178ab8d3ba981616c467dea
[kubernetes-node-windows-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.2/kubernetes-node-windows-amd64.tar.gz) | 5c29c702fbb78b53961e2afe3f51604199abcd664a270fbf2ff3b0273b983a02fbbbae4253a652ea4cd7cbef0543fe3b012c00f88e8071d9213f7cb6c4e86bda

### Container Images

All container images are available as manifest lists and support the described
architectures. It is also possible to pull a specific architecture directly by
adding the "-$ARCH" suffix  to the container image name.

name | architectures
---- | -------------
[registry.k8s.io/conformance:v1.31.0-alpha.2](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-s390x)
[registry.k8s.io/kube-apiserver:v1.31.0-alpha.2](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-s390x)
[registry.k8s.io/kube-controller-manager:v1.31.0-alpha.2](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-s390x)
[registry.k8s.io/kube-proxy:v1.31.0-alpha.2](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-s390x)
[registry.k8s.io/kube-scheduler:v1.31.0-alpha.2](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-s390x)
[registry.k8s.io/kubectl:v1.31.0-alpha.2](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-s390x)

## Changelog since v1.31.0-alpha.1

## Urgent Upgrade Notes

### (No, really, you MUST read this before you upgrade)

 - The scheduler starts to use QueueingHint registered for Pod/Updated event to determine whether unschedulable Pods update make them schedulable, when the feature gate `SchedulerQueueingHints` is enabled.
  Previously, when unschedulable Pods are updated, the scheduler always put Pods back to activeQ/backoffQ. But, actually not all updates to Pods make Pods schedulable, especially considering many scheduling constraints nowadays are immutable.
  Now, when unschedulable Pods are updated, the scheduling queue checks with QueueingHint(s) whether the update may make the pods schedulable, and requeues them to activeQ/backoffQ **only when** at least one QueueingHint(s) return Queue. 
  
  Action required for custom scheduler plugin developers:
  Plugins **have to** implement a QueueingHint for Pod/Update event if the rejection from them could be resolved by updating unscheduled Pods themselves.
  Example: suppose you develop a custom plugin that denies Pods that have a `schedulable=false` label. 
  Given Pods with a `schedulable=false` label will be schedulable if the `schedulable=false` label is removed, this plugin would implement QueueingHint for Pod/Update event that returns Queue when such label changes are made in unscheduled Pods. ([#122234](https://github.com/kubernetes/kubernetes/pull/122234), [@AxeZhan](https://github.com/AxeZhan)) [SIG Scheduling and Testing]
 
## Changes by Kind

### API Change

- Fixed incorrect "v1 Binding is deprecated in v1.6+" warning in kube-scheduler log. ([#125540](https://github.com/kubernetes/kubernetes/pull/125540), [@pohly](https://github.com/pohly)) [SIG API Machinery]

### Feature

- Feature gates for PortForward (kubectl port-forward) over WebSockets are now enabled by default (Beta).
  - Server-side feature gate: PortForwardWebsocket
  - Client-side (kubectl) feature gate: PORT_FORWARD_WEBSOCKETS environment variable
  - To turn off PortForward over WebSockets for kubectl, the environment variable feature gate must be explicitly set - PORT_FORWARD_WEBSOCKETS=false ([#125528](https://github.com/kubernetes/kubernetes/pull/125528), [@seans3](https://github.com/seans3)) [SIG API Machinery and CLI]
- Introduces new functionality to the client-go's `List` method, allowing users to enable API streaming. To activate this feature, users can set the `client-go.WatchListClient` feature gate.
  
  It is important to note that the server must support streaming for this feature to function properly. If streaming is not supported by the server, client-go will revert to using the normal `LIST` method to obtain data. ([#124509](https://github.com/kubernetes/kubernetes/pull/124509), [@p0lyn0mial](https://github.com/p0lyn0mial)) [SIG API Machinery, Auth, Instrumentation and Testing]
- Kubeadm: enabled the v1beta4 API. For a complete changelog since v1beta3 please see https://kubernetes.io/docs/reference/config-api/kubeadm-config.v1beta4/. 
  
  The API does include a few breaking changes:
  - The "extraArgs" component construct is now a list of "name"/"value" pairs instead of a string/string map. This has been done to support duplicate args where needed.
  - The "JoinConfiguration.discovery.timeout" field has been replaced by "JoinConfiguration.timeouts.discovery".
  - The "ClusterConfiguration.timeoutForControlPlane" field has been replaced by "{Init|Join}Configuration.timeouts.controlPlaneComponentHealthCheck".
  Please use the command "kubeadm config migrate" to migrate your existing v1beta3 configuration to v1beta4.
  
  v1beta3 is now marked as deprecated but will continue to be supported until version 1.34 or later.
  The storage configuration in the kube-system/kubeadm-config ConfigMap is now a v1beta4 ClusterConfiguration. ([#125029](https://github.com/kubernetes/kubernetes/pull/125029), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- LogarithmicScaleDown is now GA ([#125459](https://github.com/kubernetes/kubernetes/pull/125459), [@MinpengJin](https://github.com/MinpengJin)) [SIG Apps and Scheduling]

### Failing Test

- Fixed issue where following Windows container logs would prevent container log rotation. ([#124444](https://github.com/kubernetes/kubernetes/pull/124444), [@claudiubelu](https://github.com/claudiubelu)) [SIG Node, Testing and Windows]
- Pkg k8s.io/apiserver/pkg/storage/cacher, method (*Cacher) Wait(context.Context) error ([#125450](https://github.com/kubernetes/kubernetes/pull/125450), [@mauri870](https://github.com/mauri870)) [SIG API Machinery]

### Bug or Regression

- DRA: enhance validation for the ResourceClaimParametersReference and ResourceClassParametersReference with the following rules:
  
  1. `apiGroup`: If set, it must be a valid DNS subdomain (e.g. 'example.com').
  2. `kind` and `name`: It must be valid path segment name. It may not be '.' or '..' and it may not contain '/' and '%' characters. ([#125218](https://github.com/kubernetes/kubernetes/pull/125218), [@carlory](https://github.com/carlory)) [SIG Node]
- Kubeadm: fixed a regression where the JoinConfiguration.discovery.timeout was no longer respected and the value was always hardcoded to "5m" (5 minutes). ([#125480](https://github.com/kubernetes/kubernetes/pull/125480), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]

### Other (Cleanup or Flake)

- Removed generally available feature gate `ReadWriteOncePod`. ([#124329](https://github.com/kubernetes/kubernetes/pull/124329), [@chrishenzie](https://github.com/chrishenzie)) [SIG Storage]

## Dependencies

### Added
_Nothing has changed._

### Changed
- sigs.k8s.io/apiserver-network-proxy/konnectivity-client: v0.29.0 → v0.30.3

### Removed
_Nothing has changed._



# v1.31.0-alpha.1


## Downloads for v1.31.0-alpha.1



### Source Code

filename | sha512 hash
-------- | -----------
[kubernetes.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes.tar.gz) | c3d3b7c0f58866a09006b47ba0e7677c95451c0c5b727963ec2bb318fcf0fd94a75f14e51485dacbcf34fab2879325216d9723162e2039d09344ab75b8313fad
[kubernetes-src.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-src.tar.gz) | 16e46516d52f89b9bf623e90bab4d17708b540d67c153c0f81c42a4f6bb335f549b5c451c71701aeeb279ee3f60f1379df98bfab4d24db33a2ff7ef23b70c943

### Client Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-client-darwin-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-darwin-amd64.tar.gz) | 219fc2cfcd6da50693eca80209e6d6c7b1331c79c059126766ebdbb5dac56e8efb277bc39d0c32a4d1f4bf51445994c91ce27f291bccdda7859b4be666b2452f
[kubernetes-client-darwin-arm64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-darwin-arm64.tar.gz) | 054897580442e027c4d0c5c67769e0f98f464470147abb981b200358bcf13b134eac166845350f2e2c8460df3577982f18eafad3be698cfee6e5a4a2e088f0d3
[kubernetes-client-linux-386.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-linux-386.tar.gz) | a783ba568bbe28e0ddddcbd2c16771f2354786bcc5de4333e9d0a73a1027a8a45c2cc58c69b740db83fec12647e93df2536790df5e191d96dea914986b717ee6
[kubernetes-client-linux-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-linux-amd64.tar.gz) | f0f39dc1f8cf5dd6029afccae904cd082ed3a4da9283a4506311b0f820e50bdbe9370aaa784f382ec5cbfaa7b115ce34578801080443380f8e606fad225467f0
[kubernetes-client-linux-arm.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-linux-arm.tar.gz) | 744b69d0b0a40d8fbcb8bd582ee36da3682e189c33a780de01b320cf07eac0b215e6051f6f57ea34b9417423d0d4a42df85d72753226d53b5fe59411b096335d
[kubernetes-client-linux-arm64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-linux-arm64.tar.gz) | ebec17b4e81bfbd1789e2889435929c38976c5f054d093b964a12cf82c173a1d31c976db51c8a64bf822c17ef4ae41cef1a252bb53143094effe730601e63fe5
[kubernetes-client-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-linux-ppc64le.tar.gz) | 0b5602ec8c9c0afafe4f7c05590bdf8176ec158abb8b52e0bea026eb937945afc52aadeb4d1547fff0883b29e1aec0b92fbbae2e950a0cffa870db21674cef9e
[kubernetes-client-linux-s390x.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-linux-s390x.tar.gz) | 21b37221c9259e0c7a3fee00f4de20fbebe435755313ed0887d44989e365a67eff0450eda836e93fccf11395c89c9702a17dc494d51633f48c7bb9afe94253c4
[kubernetes-client-windows-386.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-windows-386.tar.gz) | 9e261d3ce6d640e8d43f7761777ea7d62cc0b37e709a96a1e5b691bd7fc6023804dc599edadac351dc9f9107c43bd5d6b962050a3363e5d1037036e4ab51a2ed
[kubernetes-client-windows-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-windows-amd64.tar.gz) | 53606a24ff85e011fd142a2e3b6c8cda058c77afdab6698eb488ab456bf41d299ca442c50482e00535ea6453472d987de6fd75f952febc5a33e46bb5cdf9c0ee
[kubernetes-client-windows-arm64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-client-windows-arm64.tar.gz) | f29dd44241d3363eecdcf7063cec5e6516698434c5494e922ee561b3553fbd214075cb0f4832dfadad7a894a3b9df9ee94bb4adb542feda2314d05b1b7b71f78

### Server Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-server-linux-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-server-linux-amd64.tar.gz) | 55b2c9cacb14c2a7b657079e1b2620c0580e3a01d91b0bd3f1e8b1a70e4bb59c4c361eb8aad425734fd579d6e944aedc7695082cb640a3af902dff623a565714
[kubernetes-server-linux-arm64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-server-linux-arm64.tar.gz) | 24422b969931754c7a72876d1d3ad773bdbdb42bb53ca8d2020b7987a03d20136ad5693c1aa81515b94e3ab71ed486c4b09a9d99b3ef4a7a78d8cd742f7cf9fd
[kubernetes-server-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-server-linux-ppc64le.tar.gz) | 76b6cc096ed38e0d08c1af6ee0715e0a29674eb990ee9675abb3bb9345c70469ca25b62b7babc9afdd6628d1970817d36b66a7b5890572cb0bc9519735c78599
[kubernetes-server-linux-s390x.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-server-linux-s390x.tar.gz) | 4b5a1660e1acfe3e2cb03097608c9c3c7ceedd80c9b71c22ac7572db49598d6e9bff903c8415236947ea1ba14f9595a6bbc178f15959191b92805ce5b01063c3

### Node Binaries

filename | sha512 hash
-------- | -----------
[kubernetes-node-linux-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-node-linux-amd64.tar.gz) | 98b402d2cb135af8b2d328ae452fae832e4bfe9e5ab471f277fe134411a46c5493d62def5f5af1259c977bd94b90ce8c8d5e9ba8ee1c7b7fe991316510d09e71
[kubernetes-node-linux-arm64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-node-linux-arm64.tar.gz) | 052a7ccb8ed370451d883b64cd219b803141eaef4a8498ee45c61d09eff1364b7c4d5785bc8247c9a806dee5887d53abe44e645ada2d45349a0163c3e229decd
[kubernetes-node-linux-ppc64le.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-node-linux-ppc64le.tar.gz) | 32a2cc80b367fb6a447d1b674eed220b13e03662f453c155b1752ccef72ccd55503ca73267cf782472e58771a57efc68eee4cb47520e09e6987a7183329d20fa
[kubernetes-node-linux-s390x.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-node-linux-s390x.tar.gz) | d358de45ae5566b534c9751e7acf0e577e73646d556b444020ee75a731e488ca467df1bfbc5c6a9b3e967f0ea9586bf82657cb22d569a2df69b317671dc6bcae
[kubernetes-node-windows-amd64.tar.gz](https://dl.k8s.io/v1.31.0-alpha.1/kubernetes-node-windows-amd64.tar.gz) | 95c8962439485920c0d50d85ffa037cc4dacaa61392894394759d4d9efb2525d6e1b4e6177c72eed5f55511b6f9c279795601744a1a2da2ee3cb3b518ac31c8a

### Container Images

All container images are available as manifest lists and support the described
architectures. It is also possible to pull a specific architecture directly by
adding the "-$ARCH" suffix  to the container image name.

name | architectures
---- | -------------
[registry.k8s.io/conformance:v1.31.0-alpha.1](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/conformance-s390x)
[registry.k8s.io/kube-apiserver:v1.31.0-alpha.1](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-apiserver-s390x)
[registry.k8s.io/kube-controller-manager:v1.31.0-alpha.1](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-controller-manager-s390x)
[registry.k8s.io/kube-proxy:v1.31.0-alpha.1](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-proxy-s390x)
[registry.k8s.io/kube-scheduler:v1.31.0-alpha.1](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kube-scheduler-s390x)
[registry.k8s.io/kubectl:v1.31.0-alpha.1](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl) | [amd64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-amd64), [arm64](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-arm64), [ppc64le](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-ppc64le), [s390x](https://console.cloud.google.com/artifacts/docker/k8s-artifacts-prod/southamerica-east1/images/kubectl-s390x)

## Changelog since v1.30.0

## Urgent Upgrade Notes

### (No, really, you MUST read this before you upgrade)

 - Kubelet flag `--keep-terminated-pod-volumes` was removed.  This flag was deprecated in 2017. ([#122082](https://github.com/kubernetes/kubernetes/pull/122082), [@carlory](https://github.com/carlory)) [SIG Apps, Node, Storage and Testing]
 
## Changes by Kind

### Deprecation

- CephFS volume plugin ( `kubernetes.io/cephfs`) was removed in this release and the `cephfs` volume type became non-functional. Alternative is to use CephFS CSI driver (https://github.com/ceph/ceph-csi/) in your Kubernetes Cluster. A re-deployment of your application is required to use the new driver if you were using `kubernetes.io/cephfs` volume plugin before upgrading cluster version to 1.31+. ([#124544](https://github.com/kubernetes/kubernetes/pull/124544), [@carlory](https://github.com/carlory)) [SIG Node, Scalability, Storage and Testing]
- CephRBD volume plugin ( `kubernetes.io/rbd`) was removed in this release. And its csi migration support was also removed, so the `rbd` volume type became non-functional. Alternative is to use RBD CSI driver (https://github.com/ceph/ceph-csi/) in your Kubernetes Cluster. A re-deployment of your application is required to use the new driver if you were using `kubernetes.io/rbd` volume plugin before upgrading cluster version to 1.31+. ([#124546](https://github.com/kubernetes/kubernetes/pull/124546), [@carlory](https://github.com/carlory)) [SIG Node, Scalability, Scheduling, Storage and Testing]
- Kube-scheduler deprecated all non-csi volumelimit plugins and removed those from defaults plugins. 
  - AzureDiskLimits
  - CinderLimits
  - EBSLimits
  - GCEPDLimits
  
  The NodeVolumeLimits plugin can handle the same functionality as the above plugins since the above volume types are migrated to CSI.
  Please remove those plugins and replace them with the NodeVolumeLimits plugin if you explicitly use those plugins in the scheduler config.
  Those plugins will be removed in the release 1.32. ([#124500](https://github.com/kubernetes/kubernetes/pull/124500), [@carlory](https://github.com/carlory)) [SIG Scheduling and Storage]
- Kubeadm: deprecated the kubeadm `RootlessControlPlane` feature gate (previously alpha), given that the core K8s `UserNamespacesSupport` feature gate graduated to Beta in 1.30.
  Once core Kubernetes support for user namespaces is generally available and kubeadm has started to support running the control plane in userns pods, the kubeadm `RootlessControlPlane` feature gate will be removed entirely.
  Until kubeadm supports the userns functionality out of the box, users can continue using the deprecated  `RootlessControlPlane` feature gate, or  opt-in `UserNamespacesSupport` by using kubeadm patches on the static pod manifests. ([#124997](https://github.com/kubernetes/kubernetes/pull/124997), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: mark the sub-phase of 'init kubelet-finilize' called 'experimental-cert-rotation' as deprecated and print a warning if it is used directly; it will be removed in a future release. Add a replacement sub-phase 'enable-client-cert-rotation'. ([#124419](https://github.com/kubernetes/kubernetes/pull/124419), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Remove k8s.io/legacy-cloud-providers from staging ([#124767](https://github.com/kubernetes/kubernetes/pull/124767), [@carlory](https://github.com/carlory)) [SIG API Machinery, Cloud Provider and Release]
- Removed legacy cloud provider integration code (undoing a previous reverted commit) ([#124886](https://github.com/kubernetes/kubernetes/pull/124886), [@carlory](https://github.com/carlory)) [SIG Cloud Provider and Release]

### API Change

- Added the feature gates `StrictCostEnforcementForVAP` and `StrictCostEnforcementForWebhooks` to enforce the strct cost calculation for CEL extended libraries. It is strongly recommended to turn on the feature gates as early as possible. ([#124675](https://github.com/kubernetes/kubernetes/pull/124675), [@cici37](https://github.com/cici37)) [SIG API Machinery, Auth, Node and Testing]
- Component-base/logs: when compiled with Go >= 1.21, component-base will automatically configure the slog default logger together with initializing klog. ([#120696](https://github.com/kubernetes/kubernetes/pull/120696), [@pohly](https://github.com/pohly)) [SIG API Machinery, Architecture, Auth, CLI, Cloud Provider, Cluster Lifecycle, Instrumentation, Network, Storage and Testing]
- DRA: client-side validation of a ResourceHandle would have accepted a missing DriverName, whereas server-side validation then would have raised an error. ([#124075](https://github.com/kubernetes/kubernetes/pull/124075), [@pohly](https://github.com/pohly)) [SIG Apps]
- Fix Deep Copy issue in getting controller reference ([#124116](https://github.com/kubernetes/kubernetes/pull/124116), [@HiranmoyChowdhury](https://github.com/HiranmoyChowdhury)) [SIG API Machinery and Release]
- Fix the comment for the Job's managedBy field ([#124793](https://github.com/kubernetes/kubernetes/pull/124793), [@mimowo](https://github.com/mimowo)) [SIG API Machinery and Apps]
- Fixes a 1.30.0 regression in openapi descriptions of imagePullSecrets and hostAliases fields to mark the fields used as keys in those lists as either defaulted or required. ([#124553](https://github.com/kubernetes/kubernetes/pull/124553), [@pmalek](https://github.com/pmalek)) [SIG API Machinery]
- Graduate MatchLabelKeys/MismatchLabelKeys feature in PodAffinity/PodAntiAffinity to Beta ([#123638](https://github.com/kubernetes/kubernetes/pull/123638), [@sanposhiho](https://github.com/sanposhiho)) [SIG API Machinery, Apps, Scheduling and Testing]
- Graduated the `DisableNodeKubeProxyVersion` feature gate to beta. By default, the kubelet no longer attempts to set the `.status.kubeProxyVersion` field for its associated Node. ([#123845](https://github.com/kubernetes/kubernetes/pull/123845), [@HirazawaUi](https://github.com/HirazawaUi)) [SIG API Machinery, Cloud Provider, Network, Node and Testing]
- Improved scheduling performance when many nodes, and prefilter returns 1-2 nodes (e.g. daemonset)
  
  For developers of out-of-tree PostFilter plugins, note that the semantics of NodeToStatusMap are changing: A node with an absent value in the NodeToStatusMap should be interpreted as having an UnschedulableAndUnresolvable status ([#125197](https://github.com/kubernetes/kubernetes/pull/125197), [@gabesaba](https://github.com/gabesaba)) [SIG Scheduling]
- K8s.io/apimachinery/pkg/util/runtime: new calls support handling panics and errors in the context where they occur. `PanicHandlers` and `ErrorHandlers` now must accept a context parameter for that. Log output is structured instead of unstructured. ([#121970](https://github.com/kubernetes/kubernetes/pull/121970), [@pohly](https://github.com/pohly)) [SIG API Machinery and Instrumentation]
- Kube-apiserver: the `--encryption-provider-config` file is now loaded with strict deserialization, which fails if the config file contains duplicate or unknown fields. This protects against accidentally running with config files that are malformed, mis-indented, or have typos in field names, and getting unexpected behavior. When `--encryption-provider-config-automatic-reload` is used, new encryption config files that contain typos after the kube-apiserver is running are treated as invalid and the last valid config is used. ([#124912](https://github.com/kubernetes/kubernetes/pull/124912), [@enj](https://github.com/enj)) [SIG API Machinery and Auth]
- Kube-controller-manager removes deprecated command flags: --volume-host-cidr-denylist and --volume-host-allow-local-loopback ([#124017](https://github.com/kubernetes/kubernetes/pull/124017), [@carlory](https://github.com/carlory)) [SIG API Machinery, Apps, Cloud Provider and Storage]
- Kube-controller-manager: the `horizontal-pod-autoscaler-upscale-delay` and `horizontal-pod-autoscaler-downscale-delay` flags have been removed (deprecated and non-functional since v1.12) ([#124948](https://github.com/kubernetes/kubernetes/pull/124948), [@SataQiu](https://github.com/SataQiu)) [SIG API Machinery, Apps and Autoscaling]
- Support fine-grained supplemental groups policy (KEP-3619), which enables fine-grained control for supplementary groups in the first container processes. You can choose whether to include groups defined in the container image(/etc/groups) for the container's primary uid or not. ([#117842](https://github.com/kubernetes/kubernetes/pull/117842), [@everpeace](https://github.com/everpeace)) [SIG API Machinery, Apps and Node]
- The kube-proxy nodeportAddresses / --nodeport-addresses option now
  accepts the value "primary", meaning to only listen for NodePort connections
  on the node's primary IPv4 and/or IPv6 address (according to the Node object).
  This is strongly recommended, if you were not previously using
  --nodeport-addresses, to avoid surprising behavior.
  
  (This behavior is enabled by default with the nftables backend; you would
  need to explicitly request `--nodeport-addresses 0.0.0.0/0,::/0` there to get
  the traditional "listen on all interfaces" behavior.) ([#123105](https://github.com/kubernetes/kubernetes/pull/123105), [@danwinship](https://github.com/danwinship)) [SIG API Machinery, Network and Windows]

### Feature

- Add `--keep-*` flags to `kubectl debug`, which enables to control the removal of probes, labels, annotations and initContainers from copy pod. ([#123149](https://github.com/kubernetes/kubernetes/pull/123149), [@mochizuki875](https://github.com/mochizuki875)) [SIG CLI and Testing]
- Add apiserver.latency.k8s.io/apf-queue-wait annotation to the audit log to record the time spent waiting in apf queue ([#123919](https://github.com/kubernetes/kubernetes/pull/123919), [@hakuna-matatah](https://github.com/hakuna-matatah)) [SIG API Machinery]
- Add the` WatchList` method to the `rest client` in `client-go`. When used, it establishes a stream to obtain a consistent snapshot of data from the server. This method is meant to be used by the generated client. ([#122657](https://github.com/kubernetes/kubernetes/pull/122657), [@p0lyn0mial](https://github.com/p0lyn0mial)) [SIG API Machinery]
- Added `cri-client` staging repository. ([#123797](https://github.com/kubernetes/kubernetes/pull/123797), [@saschagrunert](https://github.com/saschagrunert)) [SIG API Machinery, Node, Release and Testing]
- Added flag to `kubectl logs` called `--all-pods` to get all pods from a object that uses a pod selector. ([#124732](https://github.com/kubernetes/kubernetes/pull/124732), [@cmwylie19](https://github.com/cmwylie19)) [SIG CLI and Testing]
- Added ports autocompletion for kubectl port-foward command ([#124683](https://github.com/kubernetes/kubernetes/pull/124683), [@TessaIO](https://github.com/TessaIO)) [SIG CLI]
- Added support for building Windows kube-proxy container image.
  A container image for kube-proxy on Windows can now be built with the command
  `make release-images KUBE_BUILD_WINDOWS=y`.
  The Windows kube-proxy image can be used with Windows Host Process Containers. ([#109939](https://github.com/kubernetes/kubernetes/pull/109939), [@claudiubelu](https://github.com/claudiubelu)) [SIG Windows]
- Adds completion for `kubectl set image`. ([#124592](https://github.com/kubernetes/kubernetes/pull/124592), [@ah8ad3](https://github.com/ah8ad3)) [SIG CLI]
- Allow creating ServiceAccount tokens bound to Node objects.
  This allows users to bind a service account token's validity to a named Node object, similar to Pod bound tokens.
  Use with `kubectl create token <serviceaccount-name> --bound-object-kind=Node --bound-object-node=<node-name>`. ([#125238](https://github.com/kubernetes/kubernetes/pull/125238), [@munnerz](https://github.com/munnerz)) [SIG Auth and CLI]
- CEL default compatibility environment version to updated to 1.30 so that the extended libraries added before 1.30 is available to use. ([#124779](https://github.com/kubernetes/kubernetes/pull/124779), [@cici37](https://github.com/cici37)) [SIG API Machinery]
- CEL expressions and `additionalProperties` are now allowed to be used under nested quantifiers in CRD schemas ([#124381](https://github.com/kubernetes/kubernetes/pull/124381), [@alexzielenski](https://github.com/alexzielenski)) [SIG API Machinery]
- CEL: add name formats library ([#123572](https://github.com/kubernetes/kubernetes/pull/123572), [@alexzielenski](https://github.com/alexzielenski)) [SIG API Machinery]
- Checking etcd version to warn about deprecated etcd versions if `ConsistentListFromCache` is enabled. ([#124612](https://github.com/kubernetes/kubernetes/pull/124612), [@ah8ad3](https://github.com/ah8ad3)) [SIG API Machinery]
- Client-go/reflector: warns when the bookmark event for initial events hasn't been received ([#124614](https://github.com/kubernetes/kubernetes/pull/124614), [@p0lyn0mial](https://github.com/p0lyn0mial)) [SIG API Machinery]
- Custom resource field selectors are now in beta and enabled by default. Check out https://github.com/kubernetes/enhancements/issues/4358 for more details. ([#124681](https://github.com/kubernetes/kubernetes/pull/124681), [@jpbetz](https://github.com/jpbetz)) [SIG API Machinery, Auth and Testing]
- Dependencies: start using registry.k8s.io/pause:3.10 ([#125112](https://github.com/kubernetes/kubernetes/pull/125112), [@neolit123](https://github.com/neolit123)) [SIG CLI, Cloud Provider, Cluster Lifecycle, Node, Release, Testing and Windows]
- Graduated support for CDI device IDs to general availability. The `DevicePluginCDIDevices` feature gate is now enabled unconditionally. ([#123315](https://github.com/kubernetes/kubernetes/pull/123315), [@bart0sh](https://github.com/bart0sh)) [SIG Node]
- Kube-apiserver: http/2 serving can be disabled with a `--disable-http2-serving` flag ([#122176](https://github.com/kubernetes/kubernetes/pull/122176), [@slashpai](https://github.com/slashpai)) [SIG API Machinery]
- Kube-proxy's nftables mode (--proxy-mode=nftables) is now beta and available by default.
  
  FIXME ADD MORE HERE BEFORE THE RELEASE, DOCS LINKS AND STUFF ([#124383](https://github.com/kubernetes/kubernetes/pull/124383), [@danwinship](https://github.com/danwinship)) [SIG Cloud Provider and Network]
- Kube-scheduler implements scheduling hints for the CSILimit plugin.
  The scheduling hints allow the scheduler to retry scheduling a Pod that was previously rejected by the CSILimit plugin if a deleted pod has a PVC from the same driver. ([#121508](https://github.com/kubernetes/kubernetes/pull/121508), [@utam0k](https://github.com/utam0k)) [SIG Scheduling and Storage]
- Kube-scheduler implements scheduling hints for the InterPodAffinity plugin.
  The scheduling hints allow the scheduler to retry scheduling a Pod
  that was previously rejected by the InterPodAffinity plugin if create/delete/update a related Pod or a node which matches the pod affinity. ([#122471](https://github.com/kubernetes/kubernetes/pull/122471), [@nayihz](https://github.com/nayihz)) [SIG Scheduling and Testing]
- Kubeadm: during "upgrade" , if the "etcd.yaml" static pod does not need upgrade, still consider rotating the etcd certificates and restarting the etcd static pod if the "kube-apiserver.yaml" manifest is to be upgraded and if certificate renewal is not disabled. ([#124688](https://github.com/kubernetes/kubernetes/pull/124688), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: enhance the "patches" functionality to be able to patch coredns deployment. The new patch target is called "corednsdeployment" (e.g. patch file "corednsdeployment+json.json"). This makes it possible to apply custom patches to coredns deployment during "init" and "upgrade". ([#124820](https://github.com/kubernetes/kubernetes/pull/124820), [@SataQiu](https://github.com/SataQiu)) [SIG Cluster Lifecycle]
- Kubeadm: mark the flag "--experimental-output' as deprecated (it will be removed in a future release) and add a new flag '--output" that serves the same purpose. Affected commands are - "kubeadm config images list", "kubeadm token list", "kubeadm upgade plan", "kubeadm certs check-expiration". ([#124393](https://github.com/kubernetes/kubernetes/pull/124393), [@carlory](https://github.com/carlory)) [SIG Cluster Lifecycle]
- Kubeadm: switch to using the new etcd endpoints introduced in 3.5.11 - /livez (for liveness probe) and /readyz (for readyness and startup probe). With this change it is no longer possible to deploy a custom etcd version older than 3.5.11 with kubeadm 1.31. If so, please upgrade. ([#124465](https://github.com/kubernetes/kubernetes/pull/124465), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: switched kubeadm to start using the CRI client library instead of shelling out of the `crictl` binary
  for actions against a CRI endpoint. The kubeadm deb/rpm packages will continue to install the `cri-tools`
  package for one more release, but in you must adapt your scripts to install `crictl` manually from
  https://github.com/kubernetes-sigs/cri-tools/releases or a different location.
  
  The `kubeadm` package will stop depending on the `cri-tools` package in Kubernetes 1.32, which means that
  installing `kubeadm` will no longer automatically ensure installation of `crictl`. ([#124685](https://github.com/kubernetes/kubernetes/pull/124685), [@saschagrunert](https://github.com/saschagrunert)) [SIG Cluster Lifecycle]
- Kubeadm: use output/v1alpha3 to print structural output for the commands "kubeadm config images list" and "kubeadm token list". ([#124464](https://github.com/kubernetes/kubernetes/pull/124464), [@carlory](https://github.com/carlory)) [SIG Cluster Lifecycle]
- Kubelet server can now dynamically load certificate files ([#124574](https://github.com/kubernetes/kubernetes/pull/124574), [@zhangweikop](https://github.com/zhangweikop)) [SIG Auth and Node]
- Kubelet will not restart the container when fields other than image in the pod spec change. ([#124220](https://github.com/kubernetes/kubernetes/pull/124220), [@HirazawaUi](https://github.com/HirazawaUi)) [SIG Node]
- Kubemark: adds two flags, `--kube-api-qps` and `--kube-api-burst` ([#124147](https://github.com/kubernetes/kubernetes/pull/124147), [@devincd](https://github.com/devincd)) [SIG Scalability]
- Kubernetes is now built with go 1.22.3 ([#124828](https://github.com/kubernetes/kubernetes/pull/124828), [@cpanato](https://github.com/cpanato)) [SIG Release and Testing]
- Kubernetes is now built with go 1.22.4 ([#125363](https://github.com/kubernetes/kubernetes/pull/125363), [@cpanato](https://github.com/cpanato)) [SIG Architecture, Cloud Provider, Release, Storage and Testing]
- Pause: add a -v flag to the Windows variant of the pause binary, which prints the version of pause and exits. The Linux pause already has the flag. ([#125067](https://github.com/kubernetes/kubernetes/pull/125067), [@neolit123](https://github.com/neolit123)) [SIG Windows]
- Promoted `generateName` retries to beta, and made the `NameGenerationRetries` feature gate
  enabled by default.
  You can read https://kep.k8s.io/4420 for more details. ([#124673](https://github.com/kubernetes/kubernetes/pull/124673), [@jpbetz](https://github.com/jpbetz)) [SIG API Machinery]
- Scheduler changes its logic of calculating `evaluatedNodes` from "contains the number of nodes that filtered out by PreFilterResult and Filter plugins" to "the number of nodes filtered out by Filter plugins only". ([#124735](https://github.com/kubernetes/kubernetes/pull/124735), [@AxeZhan](https://github.com/AxeZhan)) [SIG Scheduling]
- Services implement a field selector for the ClusterIP and Type fields.
  Kubelet uses the fieldselector on Services to avoid watching for Headless Services and reduce the memory consumption. ([#123905](https://github.com/kubernetes/kubernetes/pull/123905), [@aojea](https://github.com/aojea)) [SIG Apps, Node and Testing]
- The iptables mode of kube-proxy now tracks accepted packets that are destined for node-ports on localhost by introducing `kubeproxy_iptables_localhost_nodeports_accepted_packets_total` metric.
  This will help users to identify if they rely on iptables.localhostNodePorts feature and ulitmately help them to migrate from iptables to nftables. ([#125015](https://github.com/kubernetes/kubernetes/pull/125015), [@aroradaman](https://github.com/aroradaman)) [SIG Instrumentation, Network and Testing]
- The iptables mode of kube-proxy now tracks packets that are wrongfully marked invalid by conntrack and subsequently dropped by introducing `kubeproxy_iptables_ct_state_invalid_dropped_packets_total` metric ([#122812](https://github.com/kubernetes/kubernetes/pull/122812), [@aroradaman](https://github.com/aroradaman)) [SIG Instrumentation, Network and Testing]
- The name of CEL optional type has been changed from `optional` to `optional_type`. ([#124328](https://github.com/kubernetes/kubernetes/pull/124328), [@jiahuif](https://github.com/jiahuif)) [SIG API Machinery, Architecture, Auth, CLI, Cloud Provider, Network and Node]
- The scheduler implements QueueingHint in TaintToleration plugin, which enhances the throughput of scheduling. ([#124287](https://github.com/kubernetes/kubernetes/pull/124287), [@sanposhiho](https://github.com/sanposhiho)) [SIG Scheduling and Testing]
- The sidecar finish time will be accounted when calculating the job's finish time. ([#124942](https://github.com/kubernetes/kubernetes/pull/124942), [@AxeZhan](https://github.com/AxeZhan)) [SIG Apps]
- This PR adds tracing support to the kubelet's read-only endpoint, which currently does not have tracing. It makes use the WithPublicEndpoint option to prevent callers from influencing sampling decisions. ([#121770](https://github.com/kubernetes/kubernetes/pull/121770), [@frzifus](https://github.com/frzifus)) [SIG Node]
- Users can traverse all the pods that are in the scheduler and waiting in the permit stage through method `IterateOverWaitingPods`. In other words,  all waitingPods in scheduler can be obtained from any profiles. Before this commit, each profile could only obtain waitingPods within that profile. ([#124926](https://github.com/kubernetes/kubernetes/pull/124926), [@kerthcet](https://github.com/kerthcet)) [SIG Scheduling]

### Failing Test

- Pkg k8s.io/apiserver/pkg/storage/cacher, method (*Cacher) Wait(context.Context) error ([#125450](https://github.com/kubernetes/kubernetes/pull/125450), [@mauri870](https://github.com/mauri870)) [SIG API Machinery]
- Revert "remove legacycloudproviders from staging" ([#124864](https://github.com/kubernetes/kubernetes/pull/124864), [@carlory](https://github.com/carlory)) [SIG Release]

### Bug or Regression

- .status.terminating field now gets correctly tracked for deleted active Pods when a Job fails. ([#125175](https://github.com/kubernetes/kubernetes/pull/125175), [@dejanzele](https://github.com/dejanzele)) [SIG Apps and Testing]
- Added an extra line between two different key value pairs under data when running kubectl describe configmap ([#123597](https://github.com/kubernetes/kubernetes/pull/123597), [@siddhantvirus](https://github.com/siddhantvirus)) [SIG CLI]
- Allow parameter to be set along with proto file path ([#124281](https://github.com/kubernetes/kubernetes/pull/124281), [@fulviodenza](https://github.com/fulviodenza)) [SIG API Machinery]
- Cel: converting a quantity value into a quantity value failed. ([#123669](https://github.com/kubernetes/kubernetes/pull/123669), [@pohly](https://github.com/pohly)) [SIG API Machinery]
- Client-go/tools/record.Broadcaster: fixed automatic shutdown on WithContext cancellation ([#124635](https://github.com/kubernetes/kubernetes/pull/124635), [@pohly](https://github.com/pohly)) [SIG API Machinery]
- Do not remove the "batch.kubernetes.io/job-tracking" finalizer from a Pod, in a corner
  case scenario, when the Pod is controlled by an API object which is not a batch Job
  (e.g. when the Pod is controlled by a custom CRD). ([#124798](https://github.com/kubernetes/kubernetes/pull/124798), [@mimowo](https://github.com/mimowo)) [SIG Apps and Testing]
- Drop additional rule requirement (cronjobs/finalizers) in the roles who use kubectl create cronjobs to be backwards compatible ([#124883](https://github.com/kubernetes/kubernetes/pull/124883), [@ardaguclu](https://github.com/ardaguclu)) [SIG CLI]
- Emition of RecreatingFailedPod and RecreatingTerminatedPod events has been removed from stateful set lifecycle. ([#123809](https://github.com/kubernetes/kubernetes/pull/123809), [@atiratree](https://github.com/atiratree)) [SIG Apps and Testing]
- Endpointslices mirrored from Endpoints by the EndpointSliceMirroring controller were not reconciled if modified ([#124131](https://github.com/kubernetes/kubernetes/pull/124131), [@zyjhtangtang](https://github.com/zyjhtangtang)) [SIG Apps and Network]
- Ensure daemonset controller to count old unhealthy pods towards max unavailable budget ([#123233](https://github.com/kubernetes/kubernetes/pull/123233), [@marshallbrekka](https://github.com/marshallbrekka)) [SIG Apps]
- Fix "-kube-test-repo-list" e2e flag may not take effect ([#123587](https://github.com/kubernetes/kubernetes/pull/123587), [@huww98](https://github.com/huww98)) [SIG API Machinery, Apps, Autoscaling, CLI, Network, Node, Scheduling, Storage, Testing and Windows]
- Fix a race condition in kube-controller-manager and scheduler caused by a bug in transforming informer happening when objects were accessed during Resync operation by making the transforming function idempotent. ([#124352](https://github.com/kubernetes/kubernetes/pull/124352), [@wojtek-t](https://github.com/wojtek-t)) [SIG API Machinery and Scheduling]
- Fix a race condition in transforming informer happening when objects were accessed during Resync operation ([#124344](https://github.com/kubernetes/kubernetes/pull/124344), [@wojtek-t](https://github.com/wojtek-t)) [SIG API Machinery]
- Fix kubelet on Windows fails if a pod has SecurityContext with RunAsUser ([#125040](https://github.com/kubernetes/kubernetes/pull/125040), [@carlory](https://github.com/carlory)) [SIG Storage, Testing and Windows]
- Fix throughput when scheduling daemonset pods to reach 300 pods/s, if the configured qps allows it. ([#124714](https://github.com/kubernetes/kubernetes/pull/124714), [@sanposhiho](https://github.com/sanposhiho)) [SIG Scheduling]
- Fix: the resourceclaim controller forgot to wait for podSchedulingSynced and templatesSynced ([#124589](https://github.com/kubernetes/kubernetes/pull/124589), [@carlory](https://github.com/carlory)) [SIG Apps and Node]
- Fixed EDITOR/KUBE_EDITOR with double-quoted paths with spaces when on Windows cmd.exe. ([#112104](https://github.com/kubernetes/kubernetes/pull/112104), [@oldium](https://github.com/oldium)) [SIG CLI and Windows]
- Fixed a bug in the JSON frame reader that could cause it to retain a reference to the underlying array of the byte slice passed to Read. ([#123620](https://github.com/kubernetes/kubernetes/pull/123620), [@benluddy](https://github.com/benluddy)) [SIG API Machinery]
- Fixed a bug in the scheduler where it would crash when prefilter returns a non-existent node. ([#124933](https://github.com/kubernetes/kubernetes/pull/124933), [@AxeZhan](https://github.com/AxeZhan)) [SIG Scheduling and Testing]
- Fixed a bug where `kubectl describe` incorrectly displayed NetworkPolicy port ranges
  (showing only the starting port). ([#123316](https://github.com/kubernetes/kubernetes/pull/123316), [@jcaamano](https://github.com/jcaamano)) [SIG CLI]
- Fixed a regression where `kubelet --hostname-override` no longer worked
  correctly with an external cloud provider. ([#124516](https://github.com/kubernetes/kubernetes/pull/124516), [@danwinship](https://github.com/danwinship)) [SIG Node]
- Fixed an issue that prevents the linking of trace spans for requests that are proxied through kube-aggregator. ([#124189](https://github.com/kubernetes/kubernetes/pull/124189), [@toddtreece](https://github.com/toddtreece)) [SIG API Machinery]
- Fixed bug where kubectl get with --sort-by flag does not sort strings alphanumerically. ([#124514](https://github.com/kubernetes/kubernetes/pull/124514), [@brianpursley](https://github.com/brianpursley)) [SIG CLI]
- Fixed the format of the error indicating that a user does not have permission on the object referenced by paramRef in ValidatingAdmissionPolicyBinding. ([#124653](https://github.com/kubernetes/kubernetes/pull/124653), [@m1kola](https://github.com/m1kola)) [SIG API Machinery]
- Fixes a bug where hard evictions due to resource pressure would let the pod have the full termination grace period, instead of shutting down instantly. This bug also affected force deleted pods. Both cases now get a termination grace period of 1 second. ([#124063](https://github.com/kubernetes/kubernetes/pull/124063), [@olyazavr](https://github.com/olyazavr)) [SIG Node]
- Fixes a missing `status.` prefix on custom resource validation error messages. ([#123822](https://github.com/kubernetes/kubernetes/pull/123822), [@JoelSpeed](https://github.com/JoelSpeed)) [SIG API Machinery]
- Improved scheduling latency when many gated pods ([#124618](https://github.com/kubernetes/kubernetes/pull/124618), [@gabesaba](https://github.com/gabesaba)) [SIG Scheduling and Testing]
- Job: Fix a bug that the SuccessCriteriaMet could be added to the Job with successPolicy regardless of the featureGate enabling ([#125429](https://github.com/kubernetes/kubernetes/pull/125429), [@tenzen-y](https://github.com/tenzen-y)) [SIG Apps]
- Kube-apiserver: fixes a 1.28 regression printing pods with invalid initContainer status ([#124906](https://github.com/kubernetes/kubernetes/pull/124906), [@liggitt](https://github.com/liggitt)) [SIG Node]
- Kubeadm: allow 'kubeadm init phase certs sa' to accept the '--config' flag. ([#125396](https://github.com/kubernetes/kubernetes/pull/125396), [@Kavinraja-G](https://github.com/Kavinraja-G)) [SIG Cluster Lifecycle]
- Kubeadm: don't mount /etc/pki in kube-apisever and kube-controller-manager pods as an additional Linux system CA location. Mount /etc/pki/ca-trust and /etc/pki/tls/certs instead. /etc/ca-certificate, /usr/share/ca-certificates, /usr/local/share/ca-certificates and /etc/ssl/certs continue to be mounted. ([#124361](https://github.com/kubernetes/kubernetes/pull/124361), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: during kubelet health checks, respect the healthz address:port configured in the KubeletConfiguration instead of hardcoding localhost:10248. ([#125265](https://github.com/kubernetes/kubernetes/pull/125265), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: during the preflight check "CreateJob" of "kubeadm upgrade", check if there are no nodes where a Pod can schedule. If there are none, show a warning and skip this preflight check. This can happen in single node clusters where the only node was drained. ([#124503](https://github.com/kubernetes/kubernetes/pull/124503), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: fix a regression where the KubeletConfiguration is not properly downloaded during "kubeadm upgrade" commands from the kube-system/kubelet-config ConfigMap, resulting in the local '/var/lib/kubelet/config.yaml' file being written as a defaulted config. ([#124480](https://github.com/kubernetes/kubernetes/pull/124480), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: fixed a bug where the PublicKeysECDSA feature gate was not respected when generating kubeconfig files. ([#125388](https://github.com/kubernetes/kubernetes/pull/125388), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: improve the "IsPriviledgedUser" preflight check to not fail on certain Windows setups. ([#124665](https://github.com/kubernetes/kubernetes/pull/124665), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: stop storing the ResolverConfig in the global KubeletConfiguration and instead set it dynamically for each node ([#124038](https://github.com/kubernetes/kubernetes/pull/124038), [@SataQiu](https://github.com/SataQiu)) [SIG Cluster Lifecycle]
- Kubectl support both:
  - kubectl create secret docker-registry <NAME> --from-file=<path/to/.docker/config.json>
  - kubectl create secret docker-registry <NAME> --from-file=.dockerconfigjson=<path/to/.docker/config.json> ([#119589](https://github.com/kubernetes/kubernetes/pull/119589), [@carlory](https://github.com/carlory)) [SIG CLI]
- Kubectl: Show the Pod phase in the STATUS column as 'Failed' or 'Succeeded' when the Pod is terminated ([#122038](https://github.com/kubernetes/kubernetes/pull/122038), [@lowang-bh](https://github.com/lowang-bh)) [SIG CLI]
- Kubelet no longer crashes when a DRA driver returns a nil as part of the Node(Un)PrepareResources response instead of an empty struct (did not affect drivers written in Go, first showed up with a driver written in Rust). ([#124091](https://github.com/kubernetes/kubernetes/pull/124091), [@bitoku](https://github.com/bitoku)) [SIG Node]
- Make kubectl find `kubectl-create-subcommand` plugins also when positional arguments exists, e.g. `kubectl create subcommand arg`. ([#124123](https://github.com/kubernetes/kubernetes/pull/124123), [@sttts](https://github.com/sttts)) [SIG CLI]
- Removed admission plugin PersistentVolumeLabel. Please use https://github.com/kubernetes-sigs/cloud-pv-admission-labeler instead if you need a similar functionality. ([#124505](https://github.com/kubernetes/kubernetes/pull/124505), [@jsafrane](https://github.com/jsafrane)) [SIG API Machinery, Auth and Storage]
- StatefulSet autodelete will respect controlling owners on PVC claims as described in https://github.com/kubernetes/enhancements/pull/4375 ([#122499](https://github.com/kubernetes/kubernetes/pull/122499), [@mattcary](https://github.com/mattcary)) [SIG Apps and Testing]
- The "fake" clients generated by `client-gen` now have the same semantics on
  error as the real clients; in particular, a failed Get(), Create(), etc, no longer
  returns `nil`. (It now returns a pointer to a zero-valued object, like the real
  clients do.) This will break some downstream unit tests that were testing
  `result == nil` rather than `err != nil`, and in some cases may expose bugs
  in the underlying code that were hidden by the incorrect unit tests. ([#122892](https://github.com/kubernetes/kubernetes/pull/122892), [@danwinship](https://github.com/danwinship)) [SIG API Machinery, Auth, Cloud Provider, Instrumentation and Storage]
- The Service LoadBalancer controller was not correctly considering the service.Status new IPMode field and excluding the Ports when comparing if the status has changed, causing that changes in these fields may not update the service.Status correctly ([#125225](https://github.com/kubernetes/kubernetes/pull/125225), [@aojea](https://github.com/aojea)) [SIG Apps, Cloud Provider and Network]
- The nftables kube-proxy mode now has its own metrics rather than reporting
  metrics with "iptables" in their names. ([#124557](https://github.com/kubernetes/kubernetes/pull/124557), [@danwinship](https://github.com/danwinship)) [SIG Network and Windows]
- Updated description of default values for --healthz-bind-address and --metrics-bind-address parameters ([#123545](https://github.com/kubernetes/kubernetes/pull/123545), [@yangjunmyfm192085](https://github.com/yangjunmyfm192085)) [SIG Network]

### Other (Cleanup or Flake)

- ACTION-REQUIRED: DRA drivers using the v1alpha2 kubelet gRPC API are no longer supported and need to be updated. ([#124316](https://github.com/kubernetes/kubernetes/pull/124316), [@pohly](https://github.com/pohly)) [SIG Node and Testing]
- Build etcd image v3.5.13 ([#124026](https://github.com/kubernetes/kubernetes/pull/124026), [@liangyuanpeng](https://github.com/liangyuanpeng)) [SIG API Machinery and Etcd]
- Build etcd image v3.5.14 ([#125235](https://github.com/kubernetes/kubernetes/pull/125235), [@humblec](https://github.com/humblec)) [SIG API Machinery]
- CSI spec support has been lifted to v1.9.0 in this release ([#125150](https://github.com/kubernetes/kubernetes/pull/125150), [@humblec](https://github.com/humblec)) [SIG Storage and Testing]
- E2e.test and e2e_node.test: tests which depend on alpha or beta feature gates now have `Feature:Alpha` or `Feature:Beta` as Ginkgo labels. The inline text is `[Alpha]` or `[Beta]`, as before. ([#124350](https://github.com/kubernetes/kubernetes/pull/124350), [@pohly](https://github.com/pohly)) [SIG Testing]
- Etcd: Update to v3.5.13 ([#124027](https://github.com/kubernetes/kubernetes/pull/124027), [@liangyuanpeng](https://github.com/liangyuanpeng)) [SIG API Machinery, Cloud Provider, Cluster Lifecycle, Etcd and Testing]
- Expose apiserver_watch_cache_resource_version metric to simplify debugging problems with watchcache. ([#125377](https://github.com/kubernetes/kubernetes/pull/125377), [@wojtek-t](https://github.com/wojtek-t)) [SIG API Machinery and Instrumentation]
- Fixed a typo in the help text for the pod_scheduling_sli_duration_seconds metric in kube-scheduler ([#124221](https://github.com/kubernetes/kubernetes/pull/124221), [@arturhoo](https://github.com/arturhoo)) [SIG Instrumentation, Scheduling and Testing]
- Job-controller: the `JobReadyPods` feature flag has been removed (deprecated since v1.31) ([#125168](https://github.com/kubernetes/kubernetes/pull/125168), [@kaisoz](https://github.com/kaisoz)) [SIG Apps]
- Kubeadm: improve the warning message about the NodeSwap check which kubeadm performs on preflight. ([#125157](https://github.com/kubernetes/kubernetes/pull/125157), [@carlory](https://github.com/carlory)) [SIG Cluster Lifecycle]
- Kubeadm: only enable the klog flags that are still supported for kubeadm, rather than hiding the unwanted flags. This means that the previously unrecommended hidden flags about klog (including `--alsologtostderr`, `--log-backtrace-at`, `--log-dir`, `--logtostderr`, `--log-file`, `--log-file-max-size`, `--one-output`, `--skip-log-headers`, `--stderrthreshold` and `--vmodule`) are no longer allowed to be used. ([#125179](https://github.com/kubernetes/kubernetes/pull/125179), [@SataQiu](https://github.com/SataQiu)) [SIG Cluster Lifecycle]
- Kubeadm: remove the EXPERIMENTAL tag from the phase "kubeadm join control-plane-prepare download-certs". ([#124374](https://github.com/kubernetes/kubernetes/pull/124374), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: remove the deprecated and NO-OP "kubeadm join control-plane-join update-status"  phase. ([#124373](https://github.com/kubernetes/kubernetes/pull/124373), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubeadm: removed the deprecated output.kubeadm.k8s.io/v1alpha2 API for structured output. Please use v1alpha3 instead. ([#124496](https://github.com/kubernetes/kubernetes/pull/124496), [@carlory](https://github.com/carlory)) [SIG Cluster Lifecycle]
- Kubeadm: the deprecated `UpgradeAddonsBeforeControlPlane` featuregate has been removed, upgrade of the CoreDNS and kube-proxy addons will not be triggered until all the control plane instances have been upgraded. ([#124715](https://github.com/kubernetes/kubernetes/pull/124715), [@SataQiu](https://github.com/SataQiu)) [SIG Cluster Lifecycle]
- Kubeadm: the global --rootfs flag is now considered non-experimental. ([#124375](https://github.com/kubernetes/kubernetes/pull/124375), [@neolit123](https://github.com/neolit123)) [SIG Cluster Lifecycle]
- Kubectl describe service and ingress will now use endpointslices instead of endpoints ([#124598](https://github.com/kubernetes/kubernetes/pull/124598), [@aroradaman](https://github.com/aroradaman)) [SIG CLI and Network]
- Kubelet flags `--iptables-masquerade-bit` and `--iptables-drop-bit` were deprecated in v1.28 and have now been removed entirely. ([#122363](https://github.com/kubernetes/kubernetes/pull/122363), [@carlory](https://github.com/carlory)) [SIG Network and Node]
- Migrated the pkg/proxy to use [contextual logging](https://k8s.io/docs/concepts/cluster-administration/system-logs/#contextual-logging). ([#122979](https://github.com/kubernetes/kubernetes/pull/122979), [@fatsheep9146](https://github.com/fatsheep9146)) [SIG Network and Scalability]
- Moved remote CRI implementation from kubelet to `k8s.io/cri-client` repository. ([#124634](https://github.com/kubernetes/kubernetes/pull/124634), [@saschagrunert](https://github.com/saschagrunert)) [SIG Node, Release and Testing]
- Remove GA ServiceNodePortStaticSubrange feature gate ([#124738](https://github.com/kubernetes/kubernetes/pull/124738), [@xuzhenglun](https://github.com/xuzhenglun)) [SIG Network]
- Removed generally available feature gate `CSINodeExpandSecret`. ([#124462](https://github.com/kubernetes/kubernetes/pull/124462), [@carlory](https://github.com/carlory)) [SIG Storage]
- Removed generally available feature gate `ConsistentHTTPGetHandlers`. ([#124463](https://github.com/kubernetes/kubernetes/pull/124463), [@carlory](https://github.com/carlory)) [SIG Node]
- Removes `ENABLE_CLIENT_GO_WATCH_LIST_ALPHA` environmental variable from the reflector.
  To activate the feature set `KUBE_FEATURE_WatchListClient` environmental variable or a corresponding command line option (this works only binaries that explicitly expose it). ([#122791](https://github.com/kubernetes/kubernetes/pull/122791), [@p0lyn0mial](https://github.com/p0lyn0mial)) [SIG API Machinery and Testing]
- Removing the last remaining in-tree gcp cloud provider and credential provider. Please use the external cloud provider and credential provider from https://github.com/kubernetes/cloud-provider-gcp instead. ([#124519](https://github.com/kubernetes/kubernetes/pull/124519), [@dims](https://github.com/dims)) [SIG API Machinery, Apps, Auth, Autoscaling, Cloud Provider, Instrumentation, Network, Node, Scheduling, Storage and Testing]
- Scheduler framework: PreBind implementations are now allowed to return Pending and Unschedulable status codes. ([#125360](https://github.com/kubernetes/kubernetes/pull/125360), [@pohly](https://github.com/pohly)) [SIG Scheduling]
- The feature gate "DefaultHostNetworkHostPortsInPodTemplates" has been removed.  This behavior was deprecated in v1.28, and has had no reports of trouble since. ([#124417](https://github.com/kubernetes/kubernetes/pull/124417), [@thockin](https://github.com/thockin)) [SIG Apps]
- The feature gate "SkipReadOnlyValidationGCE" has been removed.  This gate has been active for 2 releases with no reports of issues (and was such a niche thing, we didn't expect any). ([#124210](https://github.com/kubernetes/kubernetes/pull/124210), [@thockin](https://github.com/thockin)) [SIG Apps]
- The kube-scheduler exposes /livez and /readz for health checks that are in compliance with https://kubernetes.io/docs/reference/using-api/health-checks/#api-endpoints-for-health ([#118148](https://github.com/kubernetes/kubernetes/pull/118148), [@linxiulei](https://github.com/linxiulei)) [SIG API Machinery, Scheduling and Testing]
- The kubelet is no longer able to recover from device manager state file older than 1.20. If the proper recommended upgrade flow is followed, there should be no issue. ([#123398](https://github.com/kubernetes/kubernetes/pull/123398), [@ffromani](https://github.com/ffromani)) [SIG Node and Testing]
- Update CNI Plugins to v1.5.0 ([#125113](https://github.com/kubernetes/kubernetes/pull/125113), [@bzsuni](https://github.com/bzsuni)) [SIG Cloud Provider, Network, Node and Testing]
- Updated cni-plugins to v1.4.1. ([#123894](https://github.com/kubernetes/kubernetes/pull/123894), [@saschagrunert](https://github.com/saschagrunert)) [SIG Cloud Provider, Node and Testing]
- Updated cri-tools to v1.30.0. ([#124364](https://github.com/kubernetes/kubernetes/pull/124364), [@saschagrunert](https://github.com/saschagrunert)) [SIG Cloud Provider, Node and Release]

## Dependencies

### Added
- github.com/antlr4-go/antlr/v4: [v4.13.0](https://github.com/antlr4-go/antlr/tree/v4.13.0)
- github.com/go-task/slim-sprig/v3: [v3.0.0](https://github.com/go-task/slim-sprig/tree/v3.0.0)
- go.uber.org/mock: v0.4.0
- gopkg.in/evanphx/json-patch.v4: v4.12.0

### Changed
- cloud.google.com/go/compute/metadata: v0.2.3 → v0.3.0
- cloud.google.com/go/firestore: v1.11.0 → v1.12.0
- cloud.google.com/go/storage: v1.10.0 → v1.0.0
- cloud.google.com/go: v0.110.6 → v0.110.7
- github.com/alecthomas/kingpin/v2: [v2.3.2 → v2.4.0](https://github.com/alecthomas/kingpin/compare/v2.3.2...v2.4.0)
- github.com/chzyer/readline: [2972be2 → v1.5.1](https://github.com/chzyer/readline/compare/2972be2...v1.5.1)
- github.com/container-storage-interface/spec: [v1.8.0 → v1.9.0](https://github.com/container-storage-interface/spec/compare/v1.8.0...v1.9.0)
- github.com/cpuguy83/go-md2man/v2: [v2.0.2 → v2.0.3](https://github.com/cpuguy83/go-md2man/compare/v2.0.2...v2.0.3)
- github.com/davecgh/go-spew: [v1.1.1 → d8f796a](https://github.com/davecgh/go-spew/compare/v1.1.1...d8f796a)
- github.com/fxamacker/cbor/v2: [v2.6.0 → v2.7.0-beta](https://github.com/fxamacker/cbor/compare/v2.6.0...v2.7.0-beta)
- github.com/go-openapi/swag: [v0.22.3 → v0.22.4](https://github.com/go-openapi/swag/compare/v0.22.3...v0.22.4)
- github.com/golang/glog: [v1.1.0 → v1.1.2](https://github.com/golang/glog/compare/v1.1.0...v1.1.2)
- github.com/golang/mock: [v1.6.0 → v1.3.1](https://github.com/golang/mock/compare/v1.6.0...v1.3.1)
- github.com/google/cel-go: [v0.17.8 → v0.20.1](https://github.com/google/cel-go/compare/v0.17.8...v0.20.1)
- github.com/google/pprof: [4bb14d4 → 4bfdf5a](https://github.com/google/pprof/compare/4bb14d4...4bfdf5a)
- github.com/google/uuid: [v1.3.0 → v1.3.1](https://github.com/google/uuid/compare/v1.3.0...v1.3.1)
- github.com/googleapis/gax-go/v2: [v2.11.0 → v2.0.5](https://github.com/googleapis/gax-go/compare/v2.11.0...v2.0.5)
- github.com/ianlancetaylor/demangle: [28f6c0f → bd984b5](https://github.com/ianlancetaylor/demangle/compare/28f6c0f...bd984b5)
- github.com/jstemmer/go-junit-report: [v0.9.1 → af01ea7](https://github.com/jstemmer/go-junit-report/compare/v0.9.1...af01ea7)
- github.com/matttproud/golang_protobuf_extensions: [v1.0.4 → v1.0.2](https://github.com/matttproud/golang_protobuf_extensions/compare/v1.0.4...v1.0.2)
- github.com/onsi/ginkgo/v2: [v2.15.0 → v2.19.0](https://github.com/onsi/ginkgo/compare/v2.15.0...v2.19.0)
- github.com/onsi/gomega: [v1.31.0 → v1.33.1](https://github.com/onsi/gomega/compare/v1.31.0...v1.33.1)
- github.com/pmezard/go-difflib: [v1.0.0 → 5d4384e](https://github.com/pmezard/go-difflib/compare/v1.0.0...5d4384e)
- github.com/prometheus/client_golang: [v1.16.0 → v1.19.0](https://github.com/prometheus/client_golang/compare/v1.16.0...v1.19.0)
- github.com/prometheus/client_model: [v0.4.0 → v0.6.0](https://github.com/prometheus/client_model/compare/v0.4.0...v0.6.0)
- github.com/prometheus/common: [v0.44.0 → v0.48.0](https://github.com/prometheus/common/compare/v0.44.0...v0.48.0)
- github.com/prometheus/procfs: [v0.10.1 → v0.12.0](https://github.com/prometheus/procfs/compare/v0.10.1...v0.12.0)
- github.com/rogpeppe/go-internal: [v1.10.0 → v1.11.0](https://github.com/rogpeppe/go-internal/compare/v1.10.0...v1.11.0)
- github.com/sergi/go-diff: [v1.1.0 → v1.2.0](https://github.com/sergi/go-diff/compare/v1.1.0...v1.2.0)
- github.com/sirupsen/logrus: [v1.9.0 → v1.9.3](https://github.com/sirupsen/logrus/compare/v1.9.0...v1.9.3)
- github.com/spf13/cobra: [v1.7.0 → v1.8.0](https://github.com/spf13/cobra/compare/v1.7.0...v1.8.0)
- go.etcd.io/bbolt: v1.3.8 → v1.3.9
- go.etcd.io/etcd/api/v3: v3.5.10 → v3.5.13
- go.etcd.io/etcd/client/pkg/v3: v3.5.10 → v3.5.13
- go.etcd.io/etcd/client/v2: v2.305.10 → v2.305.13
- go.etcd.io/etcd/client/v3: v3.5.10 → v3.5.13
- go.etcd.io/etcd/pkg/v3: v3.5.10 → v3.5.13
- go.etcd.io/etcd/raft/v3: v3.5.10 → v3.5.13
- go.etcd.io/etcd/server/v3: v3.5.10 → v3.5.13
- go.opentelemetry.io/contrib/instrumentation/google.golang.org/grpc/otelgrpc: v0.42.0 → v0.46.0
- go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc: v1.19.0 → v1.20.0
- go.opentelemetry.io/otel/exporters/otlp/otlptrace: v1.19.0 → v1.20.0
- go.opentelemetry.io/otel/metric: v1.19.0 → v1.20.0
- go.opentelemetry.io/otel/sdk: v1.19.0 → v1.20.0
- go.opentelemetry.io/otel/trace: v1.19.0 → v1.20.0
- go.opentelemetry.io/otel: v1.19.0 → v1.20.0
- golang.org/x/crypto: v0.21.0 → v0.23.0
- golang.org/x/exp: a9213ee → f3d0a9c
- golang.org/x/lint: 6edffad → 1621716
- golang.org/x/mod: v0.15.0 → v0.17.0
- golang.org/x/net: v0.23.0 → v0.25.0
- golang.org/x/oauth2: v0.10.0 → v0.20.0
- golang.org/x/sync: v0.6.0 → v0.7.0
- golang.org/x/sys: v0.18.0 → v0.20.0
- golang.org/x/telemetry: b75ee88 → f48c80b
- golang.org/x/term: v0.18.0 → v0.20.0
- golang.org/x/text: v0.14.0 → v0.15.0
- golang.org/x/tools: v0.18.0 → v0.21.0
- google.golang.org/api: v0.126.0 → v0.13.0
- google.golang.org/genproto/googleapis/api: 23370e0 → b8732ec
- google.golang.org/genproto: f966b18 → b8732ec
- google.golang.org/grpc: v1.58.3 → v1.59.0
- honnef.co/go/tools: v0.0.1-2020.1.4 → v0.0.1-2019.2.3
- sigs.k8s.io/knftables: v0.0.14 → v0.0.16
- sigs.k8s.io/kustomize/api: 6ce0bf3 → v0.17.2
- sigs.k8s.io/kustomize/cmd/config: v0.11.2 → v0.14.1
- sigs.k8s.io/kustomize/kustomize/v5: 6ce0bf3 → v5.4.2
- sigs.k8s.io/kustomize/kyaml: 6ce0bf3 → v0.17.1
- sigs.k8s.io/yaml: v1.3.0 → v1.4.0

### Removed
- github.com/GoogleCloudPlatform/k8s-cloud-provider: [f118173](https://github.com/GoogleCloudPlatform/k8s-cloud-provider/tree/f118173)
- github.com/antlr/antlr4/runtime/Go/antlr/v4: [8188dc5](https://github.com/antlr/antlr4/tree/runtime/Go/antlr/v4/8188dc5)
- github.com/evanphx/json-patch: [v4.12.0+incompatible](https://github.com/evanphx/json-patch/tree/v4.12.0)
- github.com/fvbommel/sortorder: [v1.1.0](https://github.com/fvbommel/sortorder/tree/v1.1.0)
- github.com/go-gl/glfw/v3.3/glfw: [6f7a984](https://github.com/go-gl/glfw/tree/v3.3/glfw/6f7a984)
- github.com/go-task/slim-sprig: [52ccab3](https://github.com/go-task/slim-sprig/tree/52ccab3)
- github.com/golang/snappy: [v0.0.3](https://github.com/golang/snappy/tree/v0.0.3)
- github.com/google/martian/v3: [v3.2.1](https://github.com/google/martian/tree/v3.2.1)
- github.com/google/s2a-go: [v0.1.7](https://github.com/google/s2a-go/tree/v0.1.7)
- github.com/googleapis/enterprise-certificate-proxy: [v0.2.3](https://github.com/googleapis/enterprise-certificate-proxy/tree/v0.2.3)
- google.golang.org/genproto/googleapis/bytestream: e85fd2c
- google.golang.org/grpc/cmd/protoc-gen-go-grpc: v1.1.0
- gopkg.in/gcfg.v1: v1.2.3
- gopkg.in/warnings.v0: v0.1.2
- rsc.io/quote/v3: v3.1.0
- rsc.io/sampler: v1.3.0