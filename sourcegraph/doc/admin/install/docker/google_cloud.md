# Install Sourcegraph with Docker on Google Cloud

This tutorial shows you how to deploy Sourcegraph to a single node running on Google Cloud.

> NOTE: Trying to decide how to deploy Sourcegraph? See [our recommendations](../index.md) for how to chose a deployment type that suits your needs.

---

## Deploy to Google Cloud VM

- [Open your Google Cloud console](https://console.cloud.google.com/compute/instances) to create a new VM instance and click **Create Instance**
- Choose an appropriate machine type (we recommend at least 2 vCPU and 7.5 GB RAM, more depending on team size and number of repositories/languages enabled)
- Choose Ubuntu 16.04 LTS as your boot disk
- Check the boxes for **Allow HTTP traffic** and **Allow HTTPS traffic** in the **Firewall** section
- Open the **Management, disks, networking, and SSH keys** dropdown section and add the following in the **Startup script** field:

  ```
  #!/bin/bash
  curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo apt-key add -
  sudo add-apt-repository "deb [arch=amd64] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable"
  sudo apt-get update
  apt-cache policy docker-ce
  sudo apt-get install -y docker-ce
  mkdir -p /root/.sourcegraph/config
  mkdir -p /root/.sourcegraph/data
  docker run -d --publish 80:7080 --publish 443:7443 --restart unless-stopped --volume /root/.sourcegraph/config:/etc/sourcegraph --volume /root/.sourcegraph/data:/var/opt/sourcegraph sourcegraph/server:3.15.1
  ```

- Create your VM, then navigate to its public IP address.

- If you have configured a DNS entry for the IP, configure `externalURL` to reflect that. (Note: `externalURL` was called `appURL` in Sourcegraph 2.13 and earlier.)

---

## Update your Sourcegraph version

To update to the most recent version of Sourcegraph (X.Y.Z), SSH into your instance and run the following:

```
docker ps # get the $CONTAINER_ID of the running sourcegraph/server container
docker rm -f $CONTAINER_ID
docker run -d ... sourcegraph/server:X.Y.Z
```

---

## Using an external database for persistence

The Docker container has its own internal PostgreSQL and Redis databases. To preserve this data when you kill and recreate the container, you can [use external databases](../../external_database.md) for persistence, such as Google Cloud's [Cloud SQL for PostgreSQL](https://cloud.google.com/sql/docs/postgres/) and [Cloud Memorystore](https://cloud.google.com/memorystore/).

> NOTE: Use of external databases requires [Sourcegraph Enterprise](https://about.sourcegraph.com/pricing).
