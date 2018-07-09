#!/usr/bin/env bats

# This file is used to test the elasticsearch init.d scripts.

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
    skip_not_sysvinit
    skip_not_dpkg_or_rpm
    export_elasticsearch_paths
}

@test "[INIT.D] remove any leftover configuration to start elasticsearch on restart" {
    # This configuration can be added with a command like:
    # $ sudo update-rc.d elasticsearch defaults 95 10
    # but we want to test that the RPM and deb _don't_ add it on its own.
    # Note that it'd be incorrect to use:
    # $ sudo update-rc.d elasticsearch disable
    # here because that'd prevent elasticsearch from installing the symlinks
    # that cause it to be started on restart.
    sudo update-rc.d -f elasticsearch remove || true
    sudo chkconfig elasticsearch off || true
}

@test "[INIT.D] install elasticsearch" {
    clean_before_test
    install_package
}

@test "[INIT.D] elasticsearch fails if startup script is not executable" {
    local INIT="/etc/init.d/elasticsearch"
    local DAEMON="$ESHOME/bin/elasticsearch"

    sudo chmod -x "$DAEMON"
    run "$INIT"
    sudo chmod +x "$DAEMON"

    [ "$status" -eq 1 ]
    [[ "$output" == *"The elasticsearch startup script does not exists or it is not executable, tried: $DAEMON"* ]]
}

@test "[INIT.D] daemon isn't enabled on restart" {
    # Rather than restart the VM which would be slow we check for the symlinks
    # that init.d uses to restart the application on startup.
    ! find /etc/rc[0123456].d | grep elasticsearch
    # Note that we don't use -iname above because that'd have to look like:
    # [ $(find /etc/rc[0123456].d -iname "elasticsearch*" | wc -l) -eq 0 ]
    # Which isn't really clearer than what we do use.
}

@test "[INIT.D] start" {
    service elasticsearch start
    wait_for_elasticsearch_status
    assert_file_exist "/var/run/elasticsearch/elasticsearch.pid"
}

@test "[INIT.D] status (running)" {
    service elasticsearch status
}

##################################
# Check that Elasticsearch is working
##################################
@test "[INIT.D] test elasticsearch" {
    run_elasticsearch_tests
}

@test "[INIT.D] restart" {
    service elasticsearch restart

    wait_for_elasticsearch_status

    service elasticsearch status
}

@test "[INIT.D] stop (running)" {
    service elasticsearch stop
}

@test "[INIT.D] status (stopped)" {
    run service elasticsearch status
    # precise returns 4, trusty 3
    [ "$status" -eq 3 ] || [ "$status" -eq 4 ]
}

@test "[INIT.D] start Elasticsearch with custom JVM options" {
    assert_file_exist $ESENVFILE
    local temp=`mktemp -d`
    cp "$ESCONFIG"/elasticsearch.yml "$temp"
    cp "$ESCONFIG"/log4j2.properties "$temp"
    touch "$temp/jvm.options"
    chown -R elasticsearch:elasticsearch "$temp"
    echo "-Xms512m" >> "$temp/jvm.options"
    echo "-Xmx512m" >> "$temp/jvm.options"
    # we have to disable Log4j from using JMX lest it will hit a security
    # manager exception before we have configured logging; this will fail
    # startup since we detect usages of logging before it is configured
    echo "-Dlog4j2.disable.jmx=true" >> "$temp/jvm.options"
    cp $ESENVFILE "$temp/elasticsearch"
    echo "ES_PATH_CONF=\"$temp\"" >> $ESENVFILE
    echo "ES_JAVA_OPTS=\"-XX:-UseCompressedOops\"" >> $ESENVFILE
    service elasticsearch start
    wait_for_elasticsearch_status
    curl -s -XGET localhost:9200/_nodes | fgrep '"heap_init_in_bytes":536870912'
    curl -s -XGET localhost:9200/_nodes | fgrep '"using_compressed_ordinary_object_pointers":"false"'
    service elasticsearch stop
    cp "$temp/elasticsearch" $ESENVFILE
}

# Simulates the behavior of a system restart:
# the PID directory is deleted by the operating system
# but it should not block ES from starting
# see https://github.com/elastic/elasticsearch/issues/11594
@test "[INIT.D] delete PID_DIR and restart" {
    rm -rf /var/run/elasticsearch

    service elasticsearch start

    wait_for_elasticsearch_status

    assert_file_exist "/var/run/elasticsearch/elasticsearch.pid"

    service elasticsearch stop
}

@test "[INIT.D] GC logs exist" {
    start_elasticsearch_service
    assert_file_exist /var/log/elasticsearch/gc.log.0.current
    stop_elasticsearch_service
}

# Ensures that if $MAX_MAP_COUNT is less than the set value on the OS
# it will be updated
@test "[INIT.D] sysctl is run when the value set is too small" {
  # intentionally a ridiculously low number
  sysctl -q -w vm.max_map_count=100
  start_elasticsearch_service
  max_map_count=$(sysctl -n vm.max_map_count)
  stop_elasticsearch_service

  [ $max_map_count = 262144 ]

}

# Ensures that if $MAX_MAP_COUNT is greater than the set vaule on the OS
# we do not attempt to update it.
@test "[INIT.D] sysctl is not run when it already has a larger or equal value set" {
  # intentionally set to the default +1
  sysctl -q -w vm.max_map_count=262145
  start_elasticsearch_service
  max_map_count=$(sysctl -n vm.max_map_count)
  stop_elasticsearch_service

  # default value +1
  [ $max_map_count = 262145 ]

}
