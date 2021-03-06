??    f      L  ?   |      ?  z   ?  ?   	  <   ?	  S   
  <   b
  c  ?
  ?    .   ?  "   ?  4   
     ?     \    {  X   ?  o   ?    J  v   L  t   ?  ?  8  ;   ?  [   9  J   ?  a   ?  ?   B  ?      ?   ?  %   u  W   ?     ?  u     4   ?  -   ?  3   ?  2        Q  *   e  .   ?  *   ?  0   ?  0     0   L  "   }     ?  *   ?  A   ?     +  )   I     s     ?      ?  (   ?     ?  `     ?   m  ?   	     ?     ?  $   ?     ?       a   0  s   ?  B     +   I  +   u  6   ?  q   ?  /   J   1   z   '   ?      ?   &   ?   %   !  (   :!  #   c!      ?!     ?!  9   ?!     "      "  #   :"  ?   ^"  H   ?"  &   *#  e   Q#  ?   ?#  E   ?$  a   ?$  ?   E%  ?   &     ?&     ?&  =   '  $   T'     y'  &   ?'  +   ?'     ?'  r   (     t(  /   ?(  ?  ?(  x   ;*  ?   ?*  8   @+  A   y+  6   ?+  a  ?+  ?  T-  -   ?.  -   #/  0   Q/     ?/  !   ?/  ?   ?/  a   ?0  r   1  ?   ?1  ?   ^2  {   ?2  ?  a3  C   =5  Z   ?5  F   ?5  Z   #6  ?   ~6  ?   '7  ?   ?7     ?8  \   ?8  !   9  m   *9  +   ?9  $   ?9  /   ?9  B   :     \:  *   t:  0   ?:  +   ?:  '   ?:  '   $;  *   L;     w;     ?;  '   ?;  E   ?;     #<     ?<     [<  &   w<  $   ?<  1   ?<     ?<  ^   =  ?   p=  ?   ?=     ~>     ?>  '   ?>     ?>     ?>  o   
?  v   z?  =   ??  ,   /@  %   \@  *   ?@  `   ?@  *   A  !   9A     [A     yA      ?A     ?A  4   ?A  "   B     'B     CB  -   \B     ?B     ?B     ?B  f   ?B  ;   BC     ~C  Q   ?C  ?   ?C  :   ?D  N   E  ?   bE  ?   F     ?F     ?F  3   ?F     ,G     HG  *   dG  /   ?G     ?G  u   ?G     HH  )   \H     	   H       -   #                  3      `       d                  C          I       A          1           >   0          !           "   (       L   %       5   J   ?   4   )   b   Z   @      f   F       =         ;              c   ^         9   [   e   M      a   ,      S   '      \          Q             .   V   T   W       B      Y          E      6      :      X   &   /       P       D   K      U   2                      _   7   ]   <   8           R          $      G              O   N   *   
   +    
		  # Show metrics for all nodes
		  kubectl top node

		  # Show metrics for a given node
		  kubectl top node NODE_NAME 
		# Get the documentation of the resource and its fields
		kubectl explain pods

		# Get the documentation of a specific field of a resource
		kubectl explain pods.spec.containers 
		# Print flags inherited by all commands
		kubectl options 
		# Print the client and server versions for the current context
		kubectl version 
		# Print the supported API versions
		kubectl api-versions 
		# Show metrics for all pods in the default namespace
		kubectl top pod

		# Show metrics for all pods in the given namespace
		kubectl top pod --namespace=NAMESPACE

		# Show metrics for a given pod and its containers
		kubectl top pod POD_NAME --containers

		# Show metrics for the pods defined by label name=myLabel
		kubectl top pod -l name=myLabel 
		Convert config files between different API versions. Both YAML
		and JSON formats are accepted.

		The command takes filename, directory, or URL as input, and convert it into format
		of version specified by --output-version flag. If target version is not specified or
		not supported, convert to latest version.

		The default output will be printed to stdout in YAML format. One can use -o option
		to change to output destination. 
		Create a namespace with the specified name. 
		Create a role with single rule. 
		Create a service account with the specified name. 
		Mark node as schedulable. 
		Mark node as unschedulable. 
		Set the latest last-applied-configuration annotations by setting it to match the contents of a file.
		This results in the last-applied-configuration being updated as though 'kubectl apply -f <file>' was run,
		without updating any other parts of the object. 
	  # Create a new namespace named my-namespace
	  kubectl create namespace my-namespace 
	  # Create a new service account named my-service-account
	  kubectl create serviceaccount my-service-account 
	Create an ExternalName service with the specified name.

	ExternalName service references to an external DNS address instead of
	only pods, which will allow application authors to reference services
	that exist off platform, on other clusters, or locally. 
	Help provides help for any command in the application.
	Simply type kubectl help [path to command] for full details. 
    # Create a new LoadBalancer service named my-lbs
    kubectl create service loadbalancer my-lbs --tcp=5678:8080 
    # Dump current cluster state to stdout
    kubectl cluster-info dump

    # Dump current cluster state to /path/to/cluster-state
    kubectl cluster-info dump --output-directory=/path/to/cluster-state

    # Dump all namespaces to stdout
    kubectl cluster-info dump --all-namespaces

    # Dump a set of namespaces to /path/to/cluster-state
    kubectl cluster-info dump --namespaces default,kube-system --output-directory=/path/to/cluster-state 
    Create a LoadBalancer service with the specified name. A comma-delimited set of quota scopes that must all match each object tracked by the quota. A comma-delimited set of resource=quantity pairs that define a hard limit. A label selector to use for this budget. Only equality-based selector requirements are supported. A label selector to use for this service. Only equality-based selector requirements are supported. If empty (the default) infer the selector from the replication controller or replica set.) Additional external IP address (not managed by Kubernetes) to accept for the service. If this IP is routed to a node, the service can be accessed by this IP in addition to its generated service IP. An inline JSON override for the generated object. If this is non-empty, it is used to override the generated object. Requires that the object supply a valid apiVersion field. Approve a certificate signing request Assign your own ClusterIP or set to 'None' for a 'headless' service (no loadbalancing). Attach to a running container ClusterIP to be assigned to the service. Leave empty to auto-allocate, or set to 'None' to create a headless service. ClusterRole this ClusterRoleBinding should reference ClusterRole this RoleBinding should reference Convert config files between different API versions Copy files and directories to and from containers. Create a TLS secret Create a namespace with the specified name Create a secret for use with a Docker registry Create a secret using specified subcommand Create a service account with the specified name Delete the specified cluster from the kubeconfig Delete the specified context from the kubeconfig Deny a certificate signing request Describe one or many contexts Display clusters defined in the kubeconfig Display merged kubeconfig settings or a specified kubeconfig file Display one or many resources Drain node in preparation for maintenance Edit a resource on the server Email for Docker registry Execute a command in a container Forward one or more local ports to a pod Help about any command If non-empty, set the session affinity for the service to this; legal values: 'None', 'ClientIP' If non-empty, the annotation update will only succeed if this is the current resource-version for the object. Only valid when specifying a single resource. If non-empty, the labels update will only succeed if this is the current resource-version for the object. Only valid when specifying a single resource. Mark node as schedulable Mark node as unschedulable Mark the provided resource as paused Modify certificate resources. Modify kubeconfig files Name or number for the port on the container that the service should direct traffic to. Optional. Only return logs after a specific date (RFC3339). Defaults to all logs. Only one of since-time / since may be used. Output shell completion code for the specified shell (bash or zsh) Password for Docker registry authentication Path to PEM encoded public key certificate. Path to private key associated with given certificate. Precondition for resource version. Requires that the current resource version match this value in order to scale. Print the client and server version information Print the list of flags inherited by all commands Print the logs for a container in a pod Resume a paused resource Role this RoleBinding should reference Run a particular image on the cluster Run a proxy to the Kubernetes API server Server location for Docker registry Set specific features on objects Set the selector on a resource Show details of a specific resource or group of resources Show the status of the rollout Synonym for --target-port The image for the container to run. The image pull policy for the container. If left empty, this value will not be specified by the client and defaulted by the server The minimum number or percentage of available pods this budget requires. The name for the newly created object. The name for the newly created object. If not specified, the name of the input resource will be used. The name of the API generator to use. There are 2 generators: 'service/v1' and 'service/v2'. The only difference between them is that service port in v1 is named 'default', while it is left unnamed in v2. Default is 'service/v2'. The network protocol for the service to be created. Default is 'TCP'. The port that the service should serve on. Copied from the resource being exposed, if unspecified The resource requirement limits for this container.  For example, 'cpu=200m,memory=512Mi'.  Note that server side components may assign limits depending on the server configuration, such as limit ranges. The resource requirement requests for this container.  For example, 'cpu=100m,memory=256Mi'.  Note that server side components may assign requests depending on the server configuration, such as limit ranges. The type of secret to create Undo a previous rollout Update resource requests/limits on objects with pod templates Update the annotations on a resource Update the labels on a resource Update the taints on one or more nodes Username for Docker registry authentication View rollout history Where to output the files.  If empty or '-' uses stdout, otherwise creates a directory hierarchy in that directory dummy restart flag) kubectl controls the Kubernetes cluster manager Project-Id-Version: gettext-go-examples-hello
Report-Msgid-Bugs-To: EMAIL
PO-Revision-Date: 2022-07-04 18:54+0800
Last-Translator: zhengjiajin <zhengjiajin@caicloud.io>
Language-Team: 
Language: zh
MIME-Version: 1.0
Content-Type: text/plain; charset=UTF-8
Content-Transfer-Encoding: 8bit
Plural-Forms: nplurals=2; plural=(n > 1);
X-Generator: Poedit 3.0.1
X-Poedit-SourceCharset: UTF-8
 
		  # 显示所有节点的指标
		  kubectl top ode

		  # 显示指定节点的指标
		  kubectl top node NODE_NAME 
		# 获取资源及其字段的文档
		kubectl explain pods

		# 获取资源指定字段的文档
		kubectl explain pods.spec.containers 
		# 输出所有命令继承的 flags
		kubectl options 
		# 输出当前客户端和服务端的版本
		kubectl version 
		# 输出支持的 API 版本
		kubectl api-versions 
		# 显示 default 命名空间下所有 Pods 的指标
		kubectl top pod

		# 显示指定命名空间下所有 Pods 的指标
		kubectl top pod --namespace=NAMESPACE

		# 显示指定 Pod 和它的容器的 metrics
		kubectl top pod POD_NAME --containers

		# 显示指定 label 为 name=myLabel 的 Pods 的 metrics
		kubectl top pod -l name=myLabel 
		在不同的 API 版本之间转换配置文件。接受 YAML
		和 JSON 格式。

		这个命令以文件名, 目录, 或者 URL 作为输入，并通过 —output-version 参数
		 转换到指定版本的格式。如果没有指定目标版本或者所指定版本
		不支持, 则转换为最新版本。

		默认以 YAML 格式输出到标准输出。可以使用 -o option
		修改目标输出的格式。 
		用给定名称创建一个命名空间。 
		创建一个具有单一规则的角色。 
		用指定的名称创建一个服务账户。 
		标记节点为可调度。 
		标记节点为不可调度。 
		设置最新的 last-applied-configuration 注解，使之匹配某文件的内容。
		这会导致 last-applied-configuration 被更新，就像执行了 kubectl apply -f <file> 一样，
		只是不会更新对象的其他部分。 
	  # 创建一个名为 my-namespace 的新命名空间
	  kubectl create namespace my-namespace 
	  # 创建一个名为 my-service-account 的新服务帐户
	  kubectl create serviceaccount my-service-account 
	创建具有指定名称的 ExternalName 服务。

	ExternalName 服务引用外部 DNS 地址而不是 Pod 地址，
	这将允许应用程序作者引用存在于平台外、其他集群上或本地的服务。 
	Help 为应用程序中的任何命令提供帮助。
	只需键入 kubectl help [命令路径] 即可获得完整的详细信息。 
    # 创建一个名称为 my-lbs 的新负载均衡服务
    kubectl create service loadbalancer my-lbs --tcp=5678:8080 
    # 导出当前的集群状态信息到标准输出
    kubectl cluster-info dump

    # 导出当前的集群状态到 /path/to/cluster-state
    kubectl cluster-info dump --output-directory=/path/to/cluster-state

    # 导出所有命名空间到标准输出
    kubectl cluster-info dump --all-namespaces

    # 导出一组命名空间到 /path/to/cluster-state
    kubectl cluster-info dump --namespaces default,kube-system --output-directory=/path/to/cluster-state 
    使用一个指定的名称创建一个 LoadBalancer 服务。 一组以逗号分隔的配额范围，必须全部匹配配额所跟踪的每个对象。 一组以逗号分隔的资源=数量对，用于定义硬性限制。 一个用于该预算的标签选择器。只支持基于等值比较的选择器要求。 用于此服务的标签选择器。仅支持基于等值比较的选择器要求。如果为空（默认），则从副本控制器或副本集中推断选择器。） 为服务所接受的其他外部 IP 地址（不由 Kubernetes 管理）。如果这个 IP 被路由到一个节点，除了其生成的服务 IP 外，还可以通过这个 IP 访问服务。 针对所生成对象的内联 JSON 覆盖。如果这一对象是非空的，将用于覆盖所生成的对象。要求对象提供有效的 apiVersion 字段。 批准一个证书签署请求 为“无头”服务（无负载平衡）分配你自己的 ClusterIP 或设置为“无。 挂接到一个运行中的容器 要分配给服务的 ClusterIP。留空表示自动分配，或设置为 “None” 以创建无头服务。 ClusterRoleBinding 应该指定 ClusterRole RoleBinding 应该指定 ClusterRole 在不同的 API 版本之间转换配置文件 将文件和目录复制到容器中或从容器中复制出来。 创建一个 TLS secret 用指定的名称创建一个命名空间 创建一个给 Docker registry 使用的 Secret 使用指定的子命令创建一个 Secret 创建一个指定名称的服务账户 从 kubeconfig 中删除指定的集群 从 kubeconfig 中删除指定的上下文 拒绝一个证书签名请求 描述一个或多个上下文 显示在 kubeconfig 中定义的集群 显示合并的 kubeconfig 配置或一个指定的 kubeconfig 文件 显示一个或多个资源 清空节点以准备维护 编辑服务器上的资源 用于 Docker 镜像库的邮件地址 在某个容器中执行一个命令 将一个或多个本地端口转发到某个 Pod 关于任何命令的帮助 如果非空，则将服务的会话亲和性设置为此值；合法值：'None'、'ClientIP' 如果非空，则只有当所给值是对象的当前资源版本时，注解更新才会成功。 仅在指定单个资源时有效。 如果非空，则标签更新只有在所给值是对象的当前资源版本时才会成功。仅在指定单个资源时有效。 标记节点为可调度 标记节点为不可调度 将所指定的资源标记为已暂停 修改证书资源。 修改 kubeconfig 文件 此为端口的名称或端口号，服务应将流量定向到容器上的这一端口。此属性为可选。 仅返回在指定日期 (RFC3339) 之后的日志。默认为所有日志。只能使用 since-time / since 之一。 为指定的 Shell(Bash 或 zsh) 输出 Shell 补全代码。 用于 Docker 镜像库身份验证的密码 PEM 编码的公钥证书的路径。 与给定证书关联的私钥的路径。 资源版本的前提条件。要求当前资源版本与此值匹配才能进行扩缩操作。 输出客户端和服务端的版本信息 输出所有命令的层级关系 打印 Pod 中容器的日志 恢复暂停的资源 RoleBinding 应该引用的 Role 在集群上运行特定镜像 运行一个指向 Kubernetes API 服务器的代理 Docker 镜像库的服务器位置 为对象设置指定特性 为资源设置选择器 显示特定资源或资源组的详细信息 显示上线的状态 --target-port 的同义词 指定容器要运行的镜像. 容器的镜像拉取策略。如果留空，该值将不由客户端指定，由服务器默认设置 此预算要求的可用 Pod 的最小数量或百分比。 新创建的对象的名称。 新创建的对象的名称。如果未指定，将使用输入资源的名称。 要使用的 API 生成器的名称。有两个生成器。'service/v1' 和 'service/v2'。它们之间唯一的区别是，v1 中的服务端口被命名为 'default'，如果在 v2 中没有指定名称。默认是 'service/v2'。 要创建的服务的网络协议。默认为 “TCP”。 服务要使用的端口。如果没有指定，则从被暴露的资源复制 这个容器的资源需求限制。例如，"cpu=200m,内存=512Mi"。请注意，服务器端的组件可能会根据服务器的配置来分配限制，例如限制范围。 这个容器的资源需求请求。例如，"cpu=200m,内存=512Mi"。请注意，服务器端的组件可能会根据服务器的配置来分配限制，例如限制范围。 要创建的 Secret 类型 撤销上一次的上线 使用 Pod 模板更新对象的资源请求/限制 更新一个资源的注解 更新某资源上的标签 更新一个或者多个节点上的污点 用于 Docker 镜像库身份验证的用户名 显示上线历史 在哪里输出文件。如果为空或 “-” 则使用标准输出，否则在该目录中创建目录层次结构 假的重启标志) kubectl 控制 Kubernetes 集群管理器 