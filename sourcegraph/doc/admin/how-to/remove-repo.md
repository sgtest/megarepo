# How to remove a repository from Sourcegraph

This document walk you through the steps of removing a repository from Sourcegraph. 

## Prerequisites

This document assumes that you have:
* site-admin level permissions on your Sourcegraph instance
* access to your Sourcegraph deployment

## Steps to remove a repository from Sourcegraph

1. Add the repository name to the [exclude list](https://docs.sourcegraph.com/admin/external_service/github#exclude) in your [code host configuration](https://docs.sourcegraph.com/admin/external_service).
1. Wait for the repository to disappear from the Repository Status Page located in your Site Admin panel.

## Remove corrupted repository data from Sourcegraph

1. Add the repository name to the [exclude list](https://docs.sourcegraph.com/admin/external_service/github#exclude) in your [code host configuration](https://docs.sourcegraph.com/admin/external_service).
1. Wait for the repository to disappear from the Repository Status Page located in your Site Admin panel.
1. Once you have confirmed the previous step has been completed, you will then exec into Gitserver (for docker-compose and kubernetes deployments) to locate the files that are associated with the repository
1. Look for a directory with the name of the repository in the Gitserver. It should be located in the following file path: `data/repos/{name-of-code-host}/{name-of-repo}`
1. Delete the directory for that repo from the previous step

## To reclone a removed repository

1. Remove the repostiroy from the [exclude list](https://docs.sourcegraph.com/admin/external_service/github#exclude)
2. The reclone process should start in the next syncing cycle
