#!/bin/bash

# This file contains some utilities to test the .deb/.rpm
# packages and the SysV/Systemd scripts.

# WARNING: This testing file must be executed as root and can
# dramatically change your system. It should only be executed
# in a throw-away VM like those made by the Vagrantfile at
# the root of the Elasticsearch source code. This should
# cause the script to fail if it is executed any other way:
[ -f /etc/is_vagrant_vm ] || {
  >&2 echo "must be run on a vagrant VM"
  exit 1
}

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

# Checks if necessary commands are available to run the tests

if [ ! -x /usr/bin/which ]; then
    echo "'which' command is mandatory to run the tests"
    exit 1
fi

if [ ! -x "`which wget 2>/dev/null`" ]; then
    echo "'wget' command is mandatory to run the tests"
    exit 1
fi

if [ ! -x "`which curl 2>/dev/null`" ]; then
    echo "'curl' command is mandatory to run the tests"
    exit 1
fi

if [ ! -x "`which pgrep 2>/dev/null`" ]; then
    echo "'pgrep' command is mandatory to run the tests"
    exit 1
fi

if [ ! -x "`which unzip 2>/dev/null`" ]; then
    echo "'unzip' command is mandatory to run the tests"
    exit 1
fi

if [ ! -x "`which tar 2>/dev/null`" ]; then
    echo "'tar' command is mandatory to run the tests"
    exit 1
fi

if [ ! -x "`which unzip 2>/dev/null`" ]; then
    echo "'unzip' command is mandatory to run the tests"
    exit 1
fi

if [ ! -x "`which java 2>/dev/null`" ]; then
    # there are some tests that move java temporarily
    if [ ! -x "`command -v java.bak 2>/dev/null`" ]; then
        echo "'java' command is mandatory to run the tests"
        exit 1
    fi
fi

# Returns 0 if the 'dpkg' command is available
is_dpkg() {
    [ -x "`which dpkg 2>/dev/null`" ]
}

# Returns 0 if the 'rpm' command is available
is_rpm() {
    [ -x "`which rpm 2>/dev/null`" ]
}

# Skip test if the 'dpkg' command is not supported
skip_not_dpkg() {
    is_dpkg || skip "dpkg is not supported"
}

# Skip test if the 'rpm' command is not supported
skip_not_rpm() {
    is_rpm || skip "rpm is not supported"
}

skip_not_dpkg_or_rpm() {
    is_dpkg || is_rpm || skip "only dpkg or rpm systems are supported"
}

# Returns 0 if the system supports Systemd
is_systemd() {
    [ -x /bin/systemctl ]
}

# Skip test if Systemd is not supported
skip_not_systemd() {
    if [ ! -x /bin/systemctl ]; then
        skip "systemd is not supported"
    fi
}

# Returns 0 if the system supports SysV
is_sysvinit() {
    [ -x "`which service 2>/dev/null`" ]
}

# Skip test if SysV is not supported
skip_not_sysvinit() {
    if [ -x "`which service 2>/dev/null`" ] && is_systemd; then
        skip "sysvinit is supported, but systemd too"
    fi
    if [ ! -x "`which service 2>/dev/null`" ]; then
        skip "sysvinit is not supported"
    fi
}

# Skip if tar is not supported
skip_not_tar_gz() {
    if [ ! -x "`which tar 2>/dev/null`" ]; then
        skip "tar is not supported"
    fi
}

# Skip if unzip is not supported
skip_not_zip() {
    if [ ! -x "`which unzip 2>/dev/null`" ]; then
        skip "unzip is not supported"
    fi
}

assert_file_exist() {
    local file="$1"
    local count=$(echo "$file" | wc -l)
    [[ "$count" == "1" ]] || {
      echo "assert_file_exist must be run on a single file at a time but was called on [$count] files: $file"
      false
    }
    if [ ! -e "$file" ]; then
        echo "Should exist: ${file} but does not"
    fi
    local file=$(readlink -m "${file}")
    [ -e "$file" ]
}

assert_file_not_exist() {
    local file="$1"
    if [ -e "$file" ]; then
        echo "Should not exist: ${file} but does"
    fi
    local file=$(readlink -m "${file}")
    [ ! -e "$file" ]
}

assert_file() {
    local file="$1"
    local type=$2
    local user=$3
    local group=$4
    local privileges=$5

    assert_file_exist "$file"

    if [ "$type" = "d" ]; then
        if [ ! -d "$file" ]; then
            echo "[$file] should be a directory but is not"
        fi
        [ -d "$file" ]
    else
        if [ ! -f "$file" ]; then
            echo "[$file] should be a regular file but is not"
        fi
        [ -f "$file" ]
    fi

    if [ "x$user" != "x" ]; then
        realuser=$(find "$file" -maxdepth 0 -printf "%u")
        if [ "$realuser" != "$user" ]; then
            echo "Expected user: $user, found $realuser [$file]"
        fi
        [ "$realuser" = "$user" ]
    fi

    if [ "x$group" != "x" ]; then
        realgroup=$(find "$file" -maxdepth 0 -printf "%g")
        if [ "$realgroup" != "$group" ]; then
            echo "Expected group: $group, found $realgroup [$file]"
        fi
        [ "$realgroup" = "$group" ]
    fi

    if [ "x$privileges" != "x" ]; then
        realprivileges=$(find "$file" -maxdepth 0 -printf "%m")
        if [ "$realprivileges" != "$privileges" ]; then
            echo "Expected privileges: $privileges, found $realprivileges [$file]"
        fi
        [ "$realprivileges" = "$privileges" ]
    fi
}

assert_module_or_plugin_directory() {
    local directory=$1
    shift

    #owner group and permissions vary depending on how es was installed
    #just make sure that everything is the same as $CONFIG_DIR, which was properly set up during install
    config_user=$(find "$ESHOME" -maxdepth 0 -printf "%u")
    config_owner=$(find "$ESHOME" -maxdepth 0 -printf "%g")

    assert_file $directory d $config_user $config_owner 755
}

assert_module_or_plugin_file() {
    local file=$1
    shift

    assert_file_exist "$(readlink -m $file)"
    assert_file $file f $config_user $config_owner 644
}

assert_output() {
    echo "$output" | grep -E "$1"
}

# Deletes everything before running a test file
clean_before_test() {

    # List of files to be deleted
    ELASTICSEARCH_TEST_FILES=("/usr/share/elasticsearch" \
                            "/etc/elasticsearch" \
                            "/var/lib/elasticsearch" \
                            "/var/log/elasticsearch" \
                            "/tmp/elasticsearch" \
                            "/etc/default/elasticsearch" \
                            "/etc/sysconfig/elasticsearch"  \
                            "/var/run/elasticsearch"  \
                            "/usr/share/doc/elasticsearch" \
                            "/usr/share/doc/elasticsearch-oss" \
                            "/tmp/elasticsearch" \
                            "/usr/lib/systemd/system/elasticsearch.conf" \
                            "/usr/lib/tmpfiles.d/elasticsearch.conf" \
                            "/usr/lib/sysctl.d/elasticsearch.conf")

    # Kills all processes of user elasticsearch
    if id elasticsearch > /dev/null 2>&1; then
        pkill -u elasticsearch 2>/dev/null || true
    fi

    # Kills all running Elasticsearch processes
    ps aux | grep -i "org.elasticsearch.bootstrap.Elasticsearch" | awk {'print $2'} | xargs kill -9 > /dev/null 2>&1 || true

    purge_elasticsearch

    # Removes user & group
    userdel elasticsearch > /dev/null 2>&1 || true
    groupdel elasticsearch > /dev/null 2>&1 || true

    # Removes all files
    for d in "${ELASTICSEARCH_TEST_FILES[@]}"; do
        if [ -e "$d" ]; then
            rm -rf "$d"
        fi
    done

    if is_systemd; then
        systemctl unmask systemd-sysctl.service
    fi
}

purge_elasticsearch() {
    # Removes RPM package
    if is_rpm; then
        rpm --quiet -e $PACKAGE_NAME > /dev/null 2>&1 || true
    fi

    if [ -x "`which yum 2>/dev/null`" ]; then
        yum remove -y $PACKAGE_NAME > /dev/null 2>&1 || true
    fi

    # Removes DEB package
    if is_dpkg; then
        dpkg --purge $PACKAGE_NAME > /dev/null 2>&1 || true
    fi

    if [ -x "`which apt-get 2>/dev/null`" ]; then
        apt-get --quiet --yes purge $PACKAGE_NAME > /dev/null 2>&1 || true
    fi
}

# Start elasticsearch and wait for it to come up with a status.
# $1 - expected status - defaults to green
start_elasticsearch_service() {
    local desiredStatus=${1:-green}
    local index=$2
    local commandLineArgs=$3

    run_elasticsearch_service 0 $commandLineArgs

    wait_for_elasticsearch_status $desiredStatus $index

    if [ -r "/tmp/elasticsearch/elasticsearch.pid" ]; then
        pid=$(cat /tmp/elasticsearch/elasticsearch.pid)
        [ "x$pid" != "x" ] && [ "$pid" -gt 0 ]
        echo "Looking for elasticsearch pid...."
        ps $pid
    elif is_systemd; then
        run systemctl is-active elasticsearch.service
        [ "$status" -eq 0 ]

        run systemctl status elasticsearch.service
        [ "$status" -eq 0 ]

    elif is_sysvinit; then
        run service elasticsearch status
        [ "$status" -eq 0 ]
    fi
}

# Start elasticsearch
# $1 expected status code
# $2 additional command line args
run_elasticsearch_service() {
    local expectedStatus=$1
    local commandLineArgs=$2
    # Set the ES_PATH_CONF setting in case we start as a service
    if [ ! -z "$ES_PATH_CONF" ] ; then
        if is_dpkg; then
            echo "ES_PATH_CONF=$ES_PATH_CONF" >> /etc/default/elasticsearch;
        elif is_rpm; then
            echo "ES_PATH_CONF=$ES_PATH_CONF" >> /etc/sysconfig/elasticsearch;
        fi
    fi

    if [ -f "/tmp/elasticsearch/bin/elasticsearch" ]; then
        # we must capture the exit code to compare so we don't want to start as background process in case we expect something other than 0
        local background=""
        local timeoutCommand=""
        if [ "$expectedStatus" = 0 ]; then
            background="-d"
        else
            timeoutCommand="timeout 60s "
        fi
        # su and the Elasticsearch init script work together to break bats.
        # sudo isolates bats enough from the init script so everything continues
        # to tick along
        run sudo -u elasticsearch bash <<BASH
# If jayatana is installed then we try to use it. Elasticsearch should ignore it even when we try.
# If it doesn't ignore it then Elasticsearch will fail to start because of security errors.
# This line is attempting to emulate the on login behavior of /usr/share/upstart/sessions/jayatana.conf
[ -f /usr/share/java/jayatanaag.jar ] && export JAVA_TOOL_OPTIONS="-javaagent:/usr/share/java/jayatanaag.jar"
# And now we can start Elasticsearch normally, in the background (-d) and with a pidfile (-p).
export ES_PATH_CONF=$ES_PATH_CONF
export ES_JAVA_OPTS=$ES_JAVA_OPTS
$timeoutCommand/tmp/elasticsearch/bin/elasticsearch $background -p /tmp/elasticsearch/elasticsearch.pid $commandLineArgs
BASH
        [ "$status" -eq "$expectedStatus" ]
    elif is_systemd; then
        run systemctl daemon-reload
        [ "$status" -eq 0 ]

        run systemctl enable elasticsearch.service
        [ "$status" -eq 0 ]

        run systemctl is-enabled elasticsearch.service
        [ "$status" -eq 0 ]

        run systemctl start elasticsearch.service
        [ "$status" -eq "$expectedStatus" ]

    elif is_sysvinit; then
        run service elasticsearch start
        [ "$status" -eq "$expectedStatus" ]
    fi
}

stop_elasticsearch_service() {
    if [ -r "/tmp/elasticsearch/elasticsearch.pid" ]; then
        pid=$(cat /tmp/elasticsearch/elasticsearch.pid)
        [ "x$pid" != "x" ] && [ "$pid" -gt 0 ]

        kill -SIGTERM $pid
    elif is_systemd; then
        run systemctl stop elasticsearch.service
        [ "$status" -eq 0 ]

        run systemctl is-active elasticsearch.service
        [ "$status" -eq 3 ]

        echo "$output" | grep -E 'inactive|failed'

    elif is_sysvinit; then
        run service elasticsearch stop
        [ "$status" -eq 0 ]

        run service elasticsearch status
        [ "$status" -ne 0 ]
    fi
}

# the default netcat packages in the distributions we test are not all compatible
# so we use /dev/tcp - a feature of bash which makes tcp connections
# http://tldp.org/LDP/abs/html/devref1.html#DEVTCP
test_port() {
    local host="$1"
    local port="$2"
    cat < /dev/null > "/dev/tcp/$host/$port"
}

describe_port() {
    local host="$1"
    local port="$2"
    if test_port "$host" "$port"; then
        echo "port $port on host $host is open"
    else
        echo "port $port on host $host is not open"
    fi
}

debug_collect_logs() {
    local es_logfile="$ESLOG/elasticsearch_server.json"
    local system_logfile='/var/log/messages'

    if [ -e "$es_logfile" ]; then
        echo "Here's the elasticsearch log:"
        cat "$es_logfile"
    else
        echo "The elasticsearch log doesn't exist at $es_logfile"
    fi

    if [ -e "$system_logfile" ]; then
        echo "Here's the tail of the log at $system_logfile:"
        tail -n20 "$system_logfile"
    else
        echo "The logfile at $system_logfile doesn't exist"
    fi

    echo "Current java processes:"
    ps aux | grep java || true

    echo "Testing if ES ports are open:"
    describe_port 127.0.0.1 9200
    describe_port 127.0.0.1 9201
}

set_debug_logging() {
    if [ "$ESCONFIG" ] && [ -d "$ESCONFIG" ] && [ -f /etc/os-release ] && (grep -qi suse /etc/os-release); then
        echo 'logger.org.elasticsearch.indices: TRACE' >> "$ESCONFIG/elasticsearch.yml"
        echo 'logger.org.elasticsearch.gateway: TRACE' >> "$ESCONFIG/elasticsearch.yml"
        echo 'logger.org.elasticsearch.cluster: DEBUG' >> "$ESCONFIG/elasticsearch.yml"
    fi
}

# Waits for Elasticsearch to reach some status.
# $1 - expected status - defaults to green
wait_for_elasticsearch_status() {
    local desiredStatus=${1:-green}
    local index=$2

    echo "Making sure elasticsearch is up..."
    wget -O - --retry-connrefused --waitretry=1 --timeout=120 --tries=120 http://localhost:9200/_cluster/health || {
        echo "Looks like elasticsearch never started"
        debug_collect_logs
        false
    }

    if [ -z "index" ]; then
      echo "Tring to connect to elasticsearch and wait for expected status $desiredStatus..."
      curl -sS "http://localhost:9200/_cluster/health?wait_for_status=$desiredStatus&timeout=60s&pretty"
    else
      echo "Trying to connect to elasticsearch and wait for expected status $desiredStatus for index $index"
      curl -sS "http://localhost:9200/_cluster/health/$index?wait_for_status=$desiredStatus&timeout=60s&pretty"
    fi
    if [ $? -eq 0 ]; then
        echo "Connected"
    else
        echo "Unable to connect to Elasticsearch"
        false
    fi

    echo "Checking that the cluster health matches the waited for status..."
    run curl -sS -XGET 'http://localhost:9200/_cat/health?h=status&v=false'
    if [ "$status" -ne 0 ]; then
        echo "error when checking cluster health. code=$status output="
        echo $output
        false
    fi
    echo $output | grep $desiredStatus || {
        echo "unexpected status:  '$output' wanted '$desiredStatus'"
        false
    }
}

# Checks the current elasticsearch version using the Info REST endpoint
# $1 - expected version
check_elasticsearch_version() {
    local version=$1
    local versionToCheck
    local major=$(echo ${version} | cut -d. -f1 )
    if [ $major -ge 7 ] ; then
        versionToCheck=$version
    else
        versionToCheck=$(echo ${version} | sed -e 's/-SNAPSHOT//')
    fi

    run curl -s localhost:9200
    [ "$status" -eq 0 ]

    echo $output | grep \"number\"\ :\ \"$versionToCheck\" || {
        echo "Expected $versionToCheck but installed an unexpected version:"
        curl -s localhost:9200
        false
    }
}

# Executes some basic Elasticsearch tests
run_elasticsearch_tests() {
    # TODO this assertion is the same the one made when waiting for
    # elasticsearch to start
    run curl -XGET 'http://localhost:9200/_cat/health?h=status&v=false'
    [ "$status" -eq 0 ]
    echo "$output" | grep -w "green"

    curl -s -H "Content-Type: application/json" -XPOST 'http://localhost:9200/library/book/1?refresh=true&pretty' -d '{
      "title": "Book #1",
      "pages": 123
    }'

    curl -s -H "Content-Type: application/json" -XPOST 'http://localhost:9200/library/book/2?refresh=true&pretty' -d '{
      "title": "Book #2",
      "pages": 456
    }'

    curl -s -XGET 'http://localhost:9200/_count?pretty' |
      grep \"count\"\ :\ 2

    curl -s -XDELETE 'http://localhost:9200/_all'
}

# Move the config directory to another directory and properly chown it.
move_config() {
    local oldConfig="$ESCONFIG"
    # The custom config directory is not under /tmp or /var/tmp because
    # systemd's private temp directory functionally means different
    # processes can have different views of what's in these directories
    export ESCONFIG="${1:-$(mktemp -p /etc -d -t 'config.XXXX')}"
    echo "Moving configuration directory from $oldConfig to $ESCONFIG"

    # Move configuration files to the new configuration directory
    mv "$oldConfig"/* "$ESCONFIG"
    chown -R elasticsearch:elasticsearch "$ESCONFIG"
    assert_file_exist "$ESCONFIG/elasticsearch.yml"
    assert_file_exist "$ESCONFIG/jvm.options"
    assert_file_exist "$ESCONFIG/log4j2.properties"
}

# permissions from the user umask with the executable bit set
executable_privileges_for_user_from_umask() {
    local user=$1
    shift

    echo $((0777 & ~$(sudo -E -u $user sh -c umask) | 0111))
}

# permissions from the user umask without the executable bit set
file_privileges_for_user_from_umask() {
    local user=$1
    shift

    echo $((0777 & ~$(sudo -E -u $user sh -c umask) & ~0111))
}

# move java to simulate it not being in the path
move_java() {
    which_java=`command -v java`
    assert_file_exist $which_java
    mv $which_java ${which_java}.bak
}

# move java back to its original location
unmove_java() {
    which_java=`command -v java.bak`
    assert_file_exist $which_java
    mv $which_java `dirname $which_java`/java
}
