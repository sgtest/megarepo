# Install Sourcegraph with Docker

It takes less than 5 minutes to run and install Sourcegraph using Docker:

<!--
  DO NOT CHANGE THIS TO A CODEBLOCK.
  We want line breaks for readability, but backslashes to escape them do not work cross-platform.
  This uses line breaks that are rendered but not copy-pasted to the clipboard.
-->

<pre class="pre-wrap"><code>docker run<span class="virtual-br"></span> --publish 7080:7080 --publish 2633:2633 --rm<span class="virtual-br"></span> --volume ~/.sourcegraph/config:/etc/sourcegraph<span class="virtual-br"></span> --volume ~/.sourcegraph/data:/var/opt/sourcegraph<span class="virtual-br"></span> sourcegraph/server:3.4.3</code></pre>

Once the server is ready (logo is displayed in the terminal), navigate to the hostname or IP address on port `7080`.  Create the admin account, then you'll be guided through setting up Sourcegraph for code searching and navigation.

<!--
TODO(ryan): Replace with updated screencast
<p class="container">
  <div style="padding:56.25% 0 0 0;position:relative;">
    <iframe src="https://player.vimeo.com/video/314926561?color=0CB6F4&title=0&byline=0&portrait=0" style="position:absolute;top:0;left:0;width:100%;height:100%;" frameborder="0" webkitallowfullscreen mozallowfullscreen allowfullscreen></iframe>
  </div>
</p>
-->

For next steps and further configuration options, visit the [site administration documentation](../../index.md).

> NOTE: If you get stuck or need help, [file an issue](https://github.com/sourcegraph/sourcegraph/issues/new?&title=Improve+Sourcegraph+quickstart+guide), [tweet (@srcgraph)](https://twitter.com/srcgraph) or [email](mailto:support@sourcegraph.com?subject=Sourcegraph%20quickstart%20guide).

## Cloud installation guides

Cloud specific Sourcegraph installation guides for AWS, Google Cloud and Digital Ocean.

- [Install Sourcegraph with Docker on AWS](../../install/docker/aws.md)
- [Install Sourcegraph with Docker on Google Cloud](../../install/docker/google_cloud.md)
- [Install Sourcegraph with Docker on DigitalOcean](../../install/docker/digitalocean.md)

## File system performance on Docker for Mac

There is a [known issue](https://github.com/docker/for-mac/issues/77) in Docker for Mac that causes slower than expected file system performance on volume mounts, which impacts the performance of search and cloning.

To achieve better performance, you can do any of the following:

- For better clone performance, clone the repository on your host machine and then [add it to Sourcegraph Server](../../repo/add.md#add-repositories-already-cloned-to-disk).
- Try adding the `:delegated` suffix the data volume mount. [Learn more](https://github.com/docker/for-mac/issues/1592).
  ```
  --volume ~/.sourcegraph/data:/var/opt/sourcegraph:delegated
  ```

## Next steps

- [Configuring Sourcegraph](../../config/index.md)
- [Upgrading Sourcegraph](../../updates.md)
- [Management console](../../management_console.md)
- [Site adminstration documentation](../../index.md)
