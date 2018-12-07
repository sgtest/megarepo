# Install Sourcegraph with Docker on DigitalOcean

<style>
div.alert-info {
    background-color: rgb(221, 241, 255);
    border-radius: 0.5em;
    padding: 0.25em 1em 0.25em 1em;
}
</style>

This tutorial shows you how to deploy Sourcegraph to a single node running on DigitalOcean.

If you're just starting out, we recommend [installing Sourcegraph locally](index.md). It takes only a few minutes and lets you try out all of the features. If you need scalability and high-availability beyond what a single-server deployment can offer, use the [Lubernetes cluster deployment option](https://github.com/sourcegraph/deploy-sourcegraph).

---

## Use the "Create Droplets" wizard

[Open your DigitalOcean dashboard](https://cloud.digitalocean.com/droplets/new) to create a new droplet

- **Choose an image -** Select the **One-click apps** tab and then choose Docker
- **Choose a size -** We recommend at least 4GB RAM and 2 CPU, more depending on team size and number of repositories/languages enabled.
- **Select additional options -** Check "User data" and paste in the following:

  ```
  #cloud-config
  repo_update: true
  repo_upgrade: all

  runcmd:
  - mkdir -p /root/.sourcegraph/config
  - mkdir -p /root/.sourcegraph/data
  - [ sh, -c, 'docker run -d --publish 80:7080 --publish 443:7443 --restart unless-stopped --volume /root/.sourcegraph/config:/etc/sourcegraph --volume /root/.sourcegraph/data:/var/opt/sourcegraph --volume /var/run/docker.sock:/var/run/docker.sock sourcegraph/server:2.13.5' ]
  ```

- Launch your instance, then navigate to its IP address.

- If you have configured a DNS entry for the IP, configure `externalURL` to reflect that. If `externalURL` has the HTTPS protocol then Sourcegraph will get a certificate via [Let's Encrypt](https://letsencrypt.org/). For more information or alternative methods, see "[Using TLS/SSL](../../tls_ssl.md)". (Note: `externalURL` was called `appURL` in Sourcegraph 2.13 and earlier.)

---

## Update your Sourcegraph version

To update to the most recent version of Sourcegraph (X.Y.Z), SSH into your instance and run the following:

```
docker ps # get the $CONTAINER_ID of the running sourcegraph/server container
docker rm -f $CONTAINER_ID
docker run -d ... sourcegraph/server:X.Y.Z
```
