#!/usr/bin/env bats

# This file is used to test the installation and removal
# of a Debian package.

# WARNING: This testing file must be executed as root and can
# dramatically change your system. It should only be executed
# in a throw-away VM like those made by the Vagrantfile at
# the root of the Elasticsearch source code. This should
# cause the script to fail if it is executed any other way:
[ -f /etc/is_vagrant_vm ] || {
  >&2 echo "must be run on a vagrant VM"
  exit 1
}

# The test case can be executed with the Bash Automated
# Testing System tool available at https://github.com/sstephenson/bats
# Thanks to Sam Stephenson!

# Licensed to Elasticsearch under one or more contributor
# license agreements. See the NOTICE file distributed with
# this work for additional information regarding copyright
# ownership. Elasticsearch licenses this file to you under
# the Apache License, Version 2.0 (the "License"); you may
# not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#    http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing,
# software distributed under the License is distributed on an
# "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
# KIND, either express or implied.  See the License for the
# specific language governing permissions and limitations
# under the License.

# Load test utilities
load $BATS_UTILS/utils.bash
load $BATS_UTILS/packages.bash
load $BATS_UTILS/plugins.bash

# Cleans everything for the 1st execution
setup() {
    skip_not_dpkg
    export_elasticsearch_paths
}

@test "[DEB] package depends on bash" {
    dpkg -I elasticsearch-oss-$(cat version).deb | grep "Depends:.*bash.*"
}

@test "[DEB] package conflicts" {
    dpkg -I elasticsearch-oss-$(cat version).deb | grep "^ Conflicts: elasticsearch$"
    dpkg -I elasticsearch-$(cat version).deb | grep "^ Conflicts: elasticsearch-oss$"
}

##################################
# Install DEB package
##################################
@test "[DEB] dpkg command is available" {
    clean_before_test
    dpkg --version
}

@test "[DEB] package is available" {
    count=$(ls elasticsearch-oss-$(cat version).deb | wc -l)
    [ "$count" -eq 1 ]
}

@test "[DEB] package is not installed" {
    run dpkg -s 'elasticsearch-oss'
    [ "$status" -eq 1 ]
}

@test "[DEB] install package" {
    dpkg -i elasticsearch-oss-$(cat version).deb
}

@test "[DEB] package is installed" {
    dpkg -s 'elasticsearch-oss'
}

@test "[DEB] verify package installation" {
    verify_package_installation
}

@test "[DEB] verify elasticsearch-plugin list runs without any plugins installed" {
    local plugins_list=`$ESHOME/bin/elasticsearch-plugin list`
    [[ -z $plugins_list ]]
}

@test "[DEB] elasticsearch isn't started by package install" {
    # Wait a second to give Elasticsearch a change to start if it is going to.
    # This isn't perfect by any means but its something.
    sleep 1
    ! ps aux | grep elasticsearch | grep java
    # You might be tempted to use jps instead of the above but that'd have to
    # look like:
    # ! sudo -u elasticsearch jps | grep -i elasticsearch
    # which isn't really easier to read than the above.
}

@test "[DEB] test elasticsearch" {
    start_elasticsearch_service
    run_elasticsearch_tests
}

@test "[DEB] verify package installation after start" {
    # Checks that the startup scripts didn't change the permissions
    verify_package_installation
}

##################################
# Uninstall DEB package
##################################
@test "[DEB] remove package" {
    dpkg -r 'elasticsearch-oss'
}

@test "[DEB] package has been removed" {
    run dpkg -s 'elasticsearch-oss'
    [ "$status" -eq 0 ]
    echo "$output" | grep -i "status" | grep -i "deinstall ok"
}

@test "[DEB] verify package removal" {
    # The removal must stop the service
    count=$(ps | grep Elasticsearch | wc -l)
    [ "$count" -eq 0 ]

    # The removal must disable the service
    # see prerm file
    if is_systemd; then
        missing_exit_code=4
        if [ $(systemctl --version | head -1 | awk '{print $2}') -lt 231 ]; then
          # systemd before version 231 used exit code 3 when the service did not exist
          missing_exit_code=3
        fi
        run systemctl status elasticsearch.service
        [ "$status" -eq $missing_exit_code ]

        run systemctl is-enabled elasticsearch.service
        [ "$status" -eq 1 ]
    fi

    # Those directories are deleted when removing the package
    # see postrm file
    assert_file_not_exist "/var/log/elasticsearch"
    assert_file_not_exist "/usr/share/elasticsearch/plugins"
    assert_file_not_exist "/usr/share/elasticsearch/modules"
    assert_file_not_exist "/var/run/elasticsearch"

    # Those directories are removed by the package manager
    assert_file_not_exist "/usr/share/elasticsearch/bin"
    assert_file_not_exist "/usr/share/elasticsearch/lib"
    assert_file_not_exist "/usr/share/elasticsearch/modules"
    assert_file_not_exist "/usr/share/elasticsearch/modules/lang-painless"

    # The configuration files are still here
    assert_file_exist "/etc/elasticsearch"
    # TODO: use ucf to handle these better for Debian-based systems
    assert_file_not_exist "/etc/elasticsearch/elasticsearch.keystore"
    assert_file_not_exist "/etc/elasticsearch/.elasticsearch.keystore.initial_md5sum"
    assert_file_exist "/etc/elasticsearch/elasticsearch.yml"
    assert_file_exist "/etc/elasticsearch/jvm.options"
    assert_file_exist "/etc/elasticsearch/log4j2.properties"

    # The env file is still here
    assert_file_exist "/etc/default/elasticsearch"

    # The service files are still here
    assert_file_exist "/etc/init.d/elasticsearch"
}

@test "[DEB] purge package" {
    # User installed scripts aren't removed so we'll just get them ourselves
    rm -rf $ESSCRIPTS
    dpkg --purge 'elasticsearch-oss'
}

@test "[DEB] verify package purge" {
    # all remaining files are deleted by the purge
    assert_file_not_exist "/etc/elasticsearch"
    assert_file_not_exist "/etc/elasticsearch/elasticsearch.keystore"
    assert_file_not_exist "/etc/elasticsearch/.elasticsearch.keystore.initial_md5sum"
    assert_file_not_exist "/etc/elasticsearch/elasticsearch.yml"
    assert_file_not_exist "/etc/elasticsearch/jvm.options"
    assert_file_not_exist "/etc/elasticsearch/log4j2.properties"

    assert_file_not_exist "/etc/default/elasticsearch"

    assert_file_not_exist "/etc/init.d/elasticsearch"
    assert_file_not_exist "/usr/lib/systemd/system/elasticsearch.service"

    assert_file_not_exist "/usr/share/elasticsearch"

    assert_file_not_exist "/usr/share/doc/elasticsearch-oss"
    assert_file_not_exist "/usr/share/doc/elasticsearch-oss/copyright"
}

@test "[DEB] package has been completly removed" {
    run dpkg -s 'elasticsearch-oss'
    [ "$status" -eq 1 ]
}

@test "[DEB] reinstall package" {
    dpkg -i elasticsearch-oss-$(cat version).deb
}

@test "[DEB] package is installed by reinstall" {
    dpkg -s 'elasticsearch-oss'
}

@test "[DEB] verify package reinstallation" {
    verify_package_installation
}

@test "[DEB] repurge package" {
    dpkg --purge 'elasticsearch-oss'
}

@test "[DEB] package has been completly removed again" {
    run dpkg -s 'elasticsearch-oss'
    [ "$status" -eq 1 ]
}
