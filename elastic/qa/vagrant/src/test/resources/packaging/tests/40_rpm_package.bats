#!/usr/bin/env bats

# This file is used to test the installation of a RPM package.

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
    skip_not_rpm
    export_elasticsearch_paths
}

@test "[RPM] package depends on bash" {
    rpm -qpR elasticsearch-oss-$(cat version).rpm | grep '/bin/bash'
}

@test "[RPM] package conflicts" {
    rpm -qp --conflicts elasticsearch-oss-$(cat version).rpm | grep "^elasticsearch\s*$"
    rpm -qp --conflicts elasticsearch-$(cat version).rpm | grep "^elasticsearch-oss\s*$"
}

##################################
# Install RPM package
##################################
@test "[RPM] rpm command is available" {
    clean_before_test
    rpm --version
}

@test "[RPM] package is available" {
    count=$(ls elasticsearch-oss-$(cat version).rpm | wc -l)
    [ "$count" -eq 1 ]
}

@test "[RPM] package is not installed" {
    run rpm -qe 'elasticsearch-oss'
    [ "$status" -eq 1 ]
}

@test "[RPM] install package" {
    rpm -i elasticsearch-oss-$(cat version).rpm
}

@test "[RPM] package is installed" {
    rpm -qe 'elasticsearch-oss'
}

@test "[RPM] verify package installation" {
    verify_package_installation
}

@test "[RPM] verify elasticsearch-plugin list runs without any plugins installed" {
    local plugins_list=`$ESHOME/bin/elasticsearch-plugin list`
    [[ -z $plugins_list ]]
}

@test "[RPM] elasticsearch isn't started by package install" {
    # Wait a second to give Elasticsearch a change to start if it is going to.
    # This isn't perfect by any means but its something.
    sleep 1
    ! ps aux | grep elasticsearch | grep java
}

@test "[RPM] test elasticsearch" {
    start_elasticsearch_service
    run_elasticsearch_tests
}

@test "[RPM] verify package installation after start" {
    # Checks that the startup scripts didn't change the permissions
    verify_package_installation
}

@test "[RPM] remove package" {
    # User installed scripts aren't removed so we'll just get them ourselves
    rm -rf $ESSCRIPTS
    rpm -e 'elasticsearch-oss'
}

@test "[RPM] package has been removed" {
    run rpm -qe 'elasticsearch-oss'
    [ "$status" -eq 1 ]
}

@test "[RPM] verify package removal" {
    # The removal must stop the service
    count=$(ps | grep Elasticsearch | wc -l)
    [ "$count" -eq 0 ]

    # The removal must disable the service
    # see prerm file
    if is_systemd; then
        run systemctl is-enabled elasticsearch.service
        [ "$status" -eq 1 ]
    fi

    # Those directories are deleted when removing the package
    # see postrm file
    assert_file_not_exist "/var/log/elasticsearch"
    assert_file_not_exist "/usr/share/elasticsearch/plugins"
    assert_file_not_exist "/var/run/elasticsearch"

    # Those directories are removed by the package manager
    assert_file_not_exist "/usr/share/elasticsearch/bin"
    assert_file_not_exist "/usr/share/elasticsearch/lib"
    assert_file_not_exist "/usr/share/elasticsearch/modules"

    assert_file_not_exist "/etc/elasticsearch"

    assert_file_not_exist "/etc/init.d/elasticsearch"
    assert_file_not_exist "/usr/lib/systemd/system/elasticsearch.service"

    assert_file_not_exist "/etc/sysconfig/elasticsearch"
}

@test "[RPM] reinstall package" {
    rpm -i elasticsearch-oss-$(cat version).rpm
}

@test "[RPM] package is installed by reinstall" {
    rpm -qe 'elasticsearch-oss'
}

@test "[RPM] verify package reinstallation" {
    verify_package_installation
}

@test "[RPM] reremove package" {
    echo foobar | "$ESHOME/bin/elasticsearch-keystore" add --stdin foo.bar
    echo "# ping" >> "/etc/elasticsearch/elasticsearch.yml"
    echo "# ping" >> "/etc/elasticsearch/jvm.options"
    echo "# ping" >> "/etc/elasticsearch/log4j2.properties"
    rpm -e 'elasticsearch-oss'
}

@test "[RPM] verify preservation" {
    # The removal must disable the service
    # see prerm file
    if is_systemd; then
        run systemctl is-enabled elasticsearch.service
        [ "$status" -eq 1 ]
    fi

    # Those directories are deleted when removing the package
    # see postrm file
    assert_file_not_exist "/var/log/elasticsearch"
    assert_file_not_exist "/usr/share/elasticsearch/plugins"
    assert_file_not_exist "/usr/share/elasticsearch/modules"
    assert_file_not_exist "/var/run/elasticsearch"

    assert_file_not_exist "/usr/share/elasticsearch/bin"
    assert_file_not_exist "/usr/share/elasticsearch/lib"
    assert_file_not_exist "/usr/share/elasticsearch/modules"
    assert_file_not_exist "/usr/share/elasticsearch/modules/lang-painless"

    assert_file_exist "/etc/elasticsearch/elasticsearch.keystore"
    assert_file_not_exist "/etc/elasticsearch/elasticsearch.yml"
    assert_file_exist "/etc/elasticsearch/elasticsearch.yml.rpmsave"
    assert_file_not_exist "/etc/elasticsearch/jvm.options"
    assert_file_exist "/etc/elasticsearch/jvm.options.rpmsave"
    assert_file_not_exist "/etc/elasticsearch/log4j2.properties"
    assert_file_exist "/etc/elasticsearch/log4j2.properties.rpmsave"

    assert_file_not_exist "/etc/init.d/elasticsearch"
    assert_file_not_exist "/usr/lib/systemd/system/elasticsearch.service"

    assert_file_not_exist "/etc/sysconfig/elasticsearch"
}

@test "[RPM] finalize package removal" {
    # cleanup
    rm -rf /etc/elasticsearch
}

@test "[RPM] package has been removed again" {
    run rpm -qe 'elasticsearch-oss'
    [ "$status" -eq 1 ]
}
