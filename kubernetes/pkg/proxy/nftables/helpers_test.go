/*
Copyright 2015 The Kubernetes Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

package nftables

import (
	"context"
	"fmt"
	"regexp"
	"runtime"
	"sort"
	"strings"
	"testing"

	"github.com/danwinship/knftables"
	"github.com/google/go-cmp/cmp"
	"github.com/lithammer/dedent"

	"k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/util/sets"
	netutils "k8s.io/utils/net"
)

// getLine returns a string containing the file and line number of the caller, if
// possible. This is useful in tests with a large number of cases - when something goes
// wrong you can find which case more easily.
func getLine() string {
	_, file, line, ok := runtime.Caller(1)
	if !ok {
		return ""
	}
	return fmt.Sprintf(" (from %s:%d)", file, line)
}

// objectOrder defines the order we sort different types into (higher = earlier); while
// not necessary just for comparison purposes, it's more intuitive in the Diff output to
// see rules/sets/maps before chains/elements.
var objectOrder = map[string]int{
	"table":   10,
	"chain":   9,
	"rule":    8,
	"set":     7,
	"map":     6,
	"element": 5,
	// anything else: 0
}

// sortNFTablesTransaction sorts an nftables transaction into a standard order for comparison
func sortNFTablesTransaction(tx string) string {
	lines := strings.Split(tx, "\n")

	// strip blank lines and comments
	for i := 0; i < len(lines); {
		if lines[i] == "" || lines[i][0] == '#' {
			lines = append(lines[:i], lines[i+1:]...)
		} else {
			i++
		}
	}

	// sort remaining lines
	sort.SliceStable(lines, func(i, j int) bool {
		li := lines[i]
		wi := strings.Split(li, " ")
		lj := lines[j]
		wj := strings.Split(lj, " ")

		// All lines will start with "add OBJECTTYPE ip kube-proxy". Everything
		// except "add table" will have an object name after the table name, and
		// "add table" will have a comment after the table name. So every line
		// should have at least 5 words.
		if len(wi) < 5 || len(wj) < 5 {
			return false
		}

		// Sort by object type first.
		if wi[1] != wj[1] {
			return objectOrder[wi[1]] >= objectOrder[wj[1]]
		}

		// Sort by object name when object type is identical.
		if wi[4] != wj[4] {
			return wi[4] < wj[4]
		}

		// Leave rules in the order they were originally added.
		if wi[1] == "rule" {
			return false
		}

		// Sort by the whole line when object type and name is identical. (e.g.,
		// individual "add rule" and "add element" lines in a chain/set/map.)
		return li < lj
	})
	return strings.Join(lines, "\n")
}

// diffNFTablesTransaction is a (testable) helper function for assertNFTablesTransactionEqual
func diffNFTablesTransaction(expected, result string) string {
	expected = sortNFTablesTransaction(expected)
	result = sortNFTablesTransaction(result)

	return cmp.Diff(expected, result)
}

// assertNFTablesTransactionEqual asserts that expected and result are equal, ignoring
// irrelevant differences.
func assertNFTablesTransactionEqual(t *testing.T, line string, expected, result string) {
	diff := diffNFTablesTransaction(expected, result)
	if diff != "" {
		t.Errorf("tables do not match%s:\ndiff:\n%s\nfull result: %+v", line, diff, result)
	}
}

// diffNFTablesChain is a (testable) helper function for assertNFTablesChainEqual
func diffNFTablesChain(nft *knftables.Fake, chain, expected string) string {
	expected = strings.TrimSpace(expected)
	result := ""
	if ch := nft.Table.Chains[chain]; ch != nil {
		for i, rule := range ch.Rules {
			if i > 0 {
				result += "\n"
			}
			result += rule.Rule
		}
	}

	return cmp.Diff(expected, result)
}

// assertNFTablesChainEqual asserts that the indicated chain in nft's table contains
// exactly the rules in expected (in that order).
func assertNFTablesChainEqual(t *testing.T, line string, nft *knftables.Fake, chain, expected string) {
	if diff := diffNFTablesChain(nft, chain, expected); diff != "" {
		t.Errorf("rules do not match%s:\ndiff:\n%s", line, diff)
	}
}

// nftablesTracer holds data used while virtually tracing a packet through a set of
// iptables rules
type nftablesTracer struct {
	nft     *knftables.Fake
	nodeIPs sets.Set[string]
	t       *testing.T

	// matches accumulates the list of rules that were matched, for debugging purposes.
	matches []string

	// outputs accumulates the list of matched terminal rule targets (endpoint
	// IP:ports, or a special target like "REJECT") and is eventually used to generate
	// the return value of tracePacket.
	outputs []string

	// markMasq tracks whether the packet has been marked for masquerading
	markMasq bool
}

// newNFTablesTracer creates an nftablesTracer. nodeIPs are the IP to treat as local node
// IPs (for determining whether rules with "fib saddr type local" or "fib daddr type
// local" match).
func newNFTablesTracer(t *testing.T, nft *knftables.Fake, nodeIPs []string) *nftablesTracer {
	return &nftablesTracer{
		nft:     nft,
		nodeIPs: sets.New(nodeIPs...),
		t:       t,
	}
}

func (tracer *nftablesTracer) addressMatches(ipStr, not, ruleAddress string) bool {
	ip := netutils.ParseIPSloppy(ipStr)
	if ip == nil {
		tracer.t.Fatalf("Bad IP in test case: %s", ipStr)
	}

	var match bool
	if strings.Contains(ruleAddress, "/") {
		_, cidr, err := netutils.ParseCIDRSloppy(ruleAddress)
		if err != nil {
			tracer.t.Errorf("Bad CIDR in kube-proxy output: %v", err)
		}
		match = cidr.Contains(ip)
	} else {
		ip2 := netutils.ParseIPSloppy(ruleAddress)
		if ip2 == nil {
			tracer.t.Errorf("Bad IP/CIDR in kube-proxy output: %s", ruleAddress)
		}
		match = ip.Equal(ip2)
	}

	if not == "!= " {
		return !match
	} else {
		return match
	}
}

// matchDestIPOnly checks an "ip daddr" against a set/map, and returns the matching
// Element, if found.
func (tracer *nftablesTracer) matchDestIPOnly(elements []*knftables.Element, destIP string) *knftables.Element {
	for _, element := range elements {
		if element.Key[0] == destIP {
			return element
		}
	}
	return nil
}

// matchDest checks an "ip daddr . meta l4proto . th dport" against a set/map, and returns
// the matching Element, if found.
func (tracer *nftablesTracer) matchDest(elements []*knftables.Element, destIP, protocol, destPort string) *knftables.Element {
	for _, element := range elements {
		if element.Key[0] == destIP && element.Key[1] == protocol && element.Key[2] == destPort {
			return element
		}
	}
	return nil
}

// matchDestAndSource checks an "ip daddr . meta l4proto . th dport . ip saddr" against a
// set/map, where the source is allowed to be a CIDR, and returns the matching Element, if
// found.
func (tracer *nftablesTracer) matchDestAndSource(elements []*knftables.Element, destIP, protocol, destPort, sourceIP string) *knftables.Element {
	for _, element := range elements {
		if element.Key[0] == destIP && element.Key[1] == protocol && element.Key[2] == destPort && tracer.addressMatches(sourceIP, "", element.Key[3]) {
			return element
		}
	}
	return nil
}

// matchDestPort checks an "meta l4proto . th dport" against a set/map, and returns the
// matching Element, if found.
func (tracer *nftablesTracer) matchDestPort(elements []*knftables.Element, protocol, destPort string) *knftables.Element {
	for _, element := range elements {
		if element.Key[0] == protocol && element.Key[1] == destPort {
			return element
		}
	}
	return nil
}

// We intentionally don't try to parse arbitrary nftables rules, as the syntax is quite
// complicated and context sensitive. (E.g., "ip daddr" could be the start of an address
// comparison, or it could be the start of a set/map lookup.) Instead, we just have
// regexps to recognize the specific pieces of rules that we create in proxier.go.
// Anything matching ignoredRegexp gets stripped out of the rule, and then what's left
// *must* match one of the cases in runChain or an error will be logged. In cases where
// the regexp doesn't end with `$`, and the matched rule succeeds against the input data,
// runChain will continue trying to match the rest of the rule. E.g., "ip daddr 10.0.0.1
// drop" would first match destAddrRegexp, and then (assuming destIP was "10.0.0.1") would
// match verdictRegexp.

var destAddrRegexp = regexp.MustCompile(`^ip6* daddr (!= )?(\S+)`)
var destAddrLocalRegexp = regexp.MustCompile(`^fib daddr type local`)
var destPortRegexp = regexp.MustCompile(`^(tcp|udp|sctp) dport (\d+)`)
var destIPOnlyLookupRegexp = regexp.MustCompile(`^ip6* daddr @(\S+)`)
var destLookupRegexp = regexp.MustCompile(`^ip6* daddr \. meta l4proto \. th dport @(\S+)`)
var destSourceLookupRegexp = regexp.MustCompile(`^ip6* daddr \. meta l4proto \. th dport \. ip6* saddr @(\S+)`)
var destPortLookupRegexp = regexp.MustCompile(`^meta l4proto \. th dport @(\S+)`)

var destDispatchRegexp = regexp.MustCompile(`^ip6* daddr \. meta l4proto \. th dport vmap @(\S+)$`)
var destPortDispatchRegexp = regexp.MustCompile(`^meta l4proto \. th dport vmap @(\S+)$`)

var sourceAddrRegexp = regexp.MustCompile(`^ip6* saddr (!= )?(\S+)`)
var sourceAddrLocalRegexp = regexp.MustCompile(`^fib saddr type local`)

var endpointVMAPRegexp = regexp.MustCompile(`^numgen random mod \d+ vmap \{(.*)\}$`)
var endpointVMapEntryRegexp = regexp.MustCompile(`\d+ : goto (\S+)`)

var masqueradeRegexp = regexp.MustCompile(`^jump ` + kubeMarkMasqChain + `$`)
var jumpRegexp = regexp.MustCompile(`^(jump|goto) (\S+)$`)
var returnRegexp = regexp.MustCompile(`^return$`)
var verdictRegexp = regexp.MustCompile(`^(drop|reject)$`)
var dnatRegexp = regexp.MustCompile(`^meta l4proto (tcp|udp|sctp) dnat to (\S+)$`)

var ignoredRegexp = regexp.MustCompile(strings.Join(
	[]string{
		// Ignore comments (which can only appear at the end of a rule).
		` *comment "[^"]*"$`,

		// The trace tests only check new connections, so for our purposes, this
		// check always succeeds (and thus can be ignored).
		`^ct state new`,

		// Likewise, this rule never matches and thus never drops anything, and so
		// can be ignored.
		`^ct state invalid drop$`,
	},
	"|",
))

// runChain runs the given packet through the rules in the given table and chain, updating
// tracer's internal state accordingly. It returns true if it hits a terminal action.
func (tracer *nftablesTracer) runChain(chname, sourceIP, protocol, destIP, destPort string) bool {
	ch := tracer.nft.Table.Chains[chname]
	if ch == nil {
		tracer.t.Errorf("unknown chain %q", chname)
		return true
	}

	for _, ruleObj := range ch.Rules {
		rule := ignoredRegexp.ReplaceAllLiteralString(ruleObj.Rule, "")
		for rule != "" {
			rule = strings.TrimLeft(rule, " ")

			// Note that the order of (some of) the cases is important. e.g.,
			// masqueradeRegexp must be checked before jumpRegexp, since
			// jumpRegexp would also match masqueradeRegexp but do the wrong
			// thing with it.

			switch {
			case destIPOnlyLookupRegexp.MatchString(rule):
				// `^ip6* daddr @(\S+)`
				// Tests whether destIP is a member of the indicated set.
				match := destIPOnlyLookupRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])
				set := match[1]
				if tracer.matchDestIPOnly(tracer.nft.Table.Sets[set].Elements, destIP) == nil {
					rule = ""
					break
				}

			case destSourceLookupRegexp.MatchString(rule):
				// `^ip6* daddr . meta l4proto . th dport . ip6* saddr @(\S+)`
				// Tests whether "destIP . protocol . destPort . sourceIP" is
				// a member of the indicated set.
				match := destSourceLookupRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])
				set := match[1]
				if tracer.matchDestAndSource(tracer.nft.Table.Sets[set].Elements, destIP, protocol, destPort, sourceIP) == nil {
					rule = ""
					break
				}

			case destLookupRegexp.MatchString(rule):
				// `^ip6* daddr . meta l4proto . th dport @(\S+)`
				// Tests whether "destIP . protocol . destPort" is a member
				// of the indicated set.
				match := destLookupRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])
				set := match[1]
				if tracer.matchDest(tracer.nft.Table.Sets[set].Elements, destIP, protocol, destPort) == nil {
					rule = ""
					break
				}

			case destPortLookupRegexp.MatchString(rule):
				// `^meta l4proto . th dport @(\S+)`
				// Tests whether "protocol . destPort" is a member of the
				// indicated set.
				match := destPortLookupRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])
				set := match[1]
				if tracer.matchDestPort(tracer.nft.Table.Sets[set].Elements, protocol, destPort) == nil {
					rule = ""
					break
				}

			case destDispatchRegexp.MatchString(rule):
				// `^ip6* daddr \. meta l4proto \. th dport vmap @(\S+)$`
				// Looks up "destIP . protocol . destPort" in the indicated
				// verdict map, and if found, runs the assocated verdict.
				match := destDispatchRegexp.FindStringSubmatch(rule)
				mapName := match[1]
				element := tracer.matchDest(tracer.nft.Table.Maps[mapName].Elements, destIP, protocol, destPort)
				if element == nil {
					rule = ""
					break
				} else {
					rule = element.Value[0]
				}

			case destPortDispatchRegexp.MatchString(rule):
				// `^meta l4proto \. th dport vmap @(\S+)$`
				// Looks up "protocol . destPort" in the indicated verdict map,
				// and if found, runs the assocated verdict.
				match := destPortDispatchRegexp.FindStringSubmatch(rule)
				mapName := match[1]
				element := tracer.matchDestPort(tracer.nft.Table.Maps[mapName].Elements, protocol, destPort)
				if element == nil {
					rule = ""
					break
				} else {
					rule = element.Value[0]
				}

			case destAddrRegexp.MatchString(rule):
				// `^ip6* daddr (!= )?(\S+)`
				// Tests whether destIP does/doesn't match a literal.
				match := destAddrRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])
				not, ip := match[1], match[2]
				if !tracer.addressMatches(destIP, not, ip) {
					rule = ""
					break
				}

			case destAddrLocalRegexp.MatchString(rule):
				// `^fib daddr type local`
				// Tests whether destIP is a local IP.
				match := destAddrLocalRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])
				if !tracer.nodeIPs.Has(destIP) {
					rule = ""
					break
				}

			case destPortRegexp.MatchString(rule):
				// `^(tcp|udp|sctp) dport (\d+)`
				// Tests whether destPort matches a literal.
				match := destPortRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])
				proto, port := match[1], match[2]
				if protocol != proto || destPort != port {
					rule = ""
					break
				}

			case sourceAddrRegexp.MatchString(rule):
				// `^ip6* saddr (!= )?(\S+)`
				// Tests whether sourceIP does/doesn't match a literal.
				match := sourceAddrRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])
				not, ip := match[1], match[2]
				if !tracer.addressMatches(sourceIP, not, ip) {
					rule = ""
					break
				}

			case sourceAddrLocalRegexp.MatchString(rule):
				// `^fib saddr type local`
				// Tests whether sourceIP is a local IP.
				match := sourceAddrLocalRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])
				if !tracer.nodeIPs.Has(sourceIP) {
					rule = ""
					break
				}

			case masqueradeRegexp.MatchString(rule):
				// `^jump mark-for-masquerade$`
				// Mark for masquerade: we just treat the jump rule itself as
				// being what creates the mark, rather than trying to handle
				// the rules inside that chain and the "masquerading" chain.
				match := jumpRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])

				tracer.matches = append(tracer.matches, ruleObj.Rule)
				tracer.markMasq = true

			case jumpRegexp.MatchString(rule):
				// `^(jump|goto) (\S+)$`
				// Jumps to another chain.
				match := jumpRegexp.FindStringSubmatch(rule)
				rule = strings.TrimPrefix(rule, match[0])
				action, destChain := match[1], match[2]

				tracer.matches = append(tracer.matches, ruleObj.Rule)
				terminated := tracer.runChain(destChain, sourceIP, protocol, destIP, destPort)
				if terminated {
					// destChain reached a terminal statement, so we
					// terminate too.
					return true
				} else if action == "goto" {
					// After a goto, return to our calling chain
					// (without terminating) rather than continuing
					// with this chain.
					return false
				}

			case verdictRegexp.MatchString(rule):
				// `^(drop|reject)$`
				// Drop/reject the packet and terminate processing.
				match := verdictRegexp.FindStringSubmatch(rule)
				verdict := match[1]

				tracer.matches = append(tracer.matches, ruleObj.Rule)
				tracer.outputs = append(tracer.outputs, strings.ToUpper(verdict))
				return true

			case returnRegexp.MatchString(rule):
				// `^return$`
				// Returns to the calling chain.
				tracer.matches = append(tracer.matches, ruleObj.Rule)
				return false

			case dnatRegexp.MatchString(rule):
				// `meta l4proto (tcp|udp|sctp) dnat to (\S+)`
				// DNAT to an endpoint IP and terminate processing.
				match := dnatRegexp.FindStringSubmatch(rule)
				destEndpoint := match[2]

				tracer.matches = append(tracer.matches, ruleObj.Rule)
				tracer.outputs = append(tracer.outputs, destEndpoint)
				return true

			case endpointVMAPRegexp.MatchString(rule):
				// `^numgen random mod \d+ vmap \{(.*)\}$`
				// Selects a random endpoint and jumps to it. For tracePacket's
				// purposes, we jump to *all* of the endpoints.
				match := endpointVMAPRegexp.FindStringSubmatch(rule)
				elements := match[1]

				for _, match = range endpointVMapEntryRegexp.FindAllStringSubmatch(elements, -1) {
					// `\d+ : goto (\S+)`
					destChain := match[1]

					tracer.matches = append(tracer.matches, ruleObj.Rule)
					// Ignore return value; we know each endpoint has a
					// terminating dnat verdict, but we want to gather all
					// of the endpoints into tracer.output.
					_ = tracer.runChain(destChain, sourceIP, protocol, destIP, destPort)
				}
				return true

			default:
				tracer.t.Errorf("unmatched rule: %s", ruleObj.Rule)
				rule = ""
			}
		}
	}

	return false
}

// tracePacket determines what would happen to a packet with the given sourceIP, destIP,
// and destPort, given the indicated iptables ruleData. nodeIPs are the local node IPs (for
// rules matching "local"). (The protocol value should be lowercase as in nftables
// rules, not uppercase as in corev1.)
//
// The return values are: an array of matched rules (for debugging), the final packet
// destinations (a comma-separated list of IPs, or one of the special targets "ACCEPT",
// "DROP", or "REJECT"), and whether the packet would be masqueraded.
func tracePacket(t *testing.T, nft *knftables.Fake, sourceIP, protocol, destIP, destPort string, nodeIPs []string) ([]string, string, bool) {
	tracer := newNFTablesTracer(t, nft, nodeIPs)

	// Collect "base chains" (ie, the chains that are run by netfilter directly rather
	// than only being run when they are jumped to). Skip postrouting because it only
	// does masquerading and we handle that separately.
	var baseChains []string
	for chname, ch := range nft.Table.Chains {
		if ch.Priority != nil && chname != "nat-postrouting" {
			baseChains = append(baseChains, chname)
		}
	}

	// Sort by priority
	sort.Slice(baseChains, func(i, j int) bool {
		// FIXME: IPv4 vs IPv6 doesn't actually matter here
		iprio, _ := knftables.ParsePriority(knftables.IPv4Family, string(*nft.Table.Chains[baseChains[i]].Priority))
		jprio, _ := knftables.ParsePriority(knftables.IPv4Family, string(*nft.Table.Chains[baseChains[j]].Priority))
		return iprio < jprio
	})

	for _, chname := range baseChains {
		terminated := tracer.runChain(chname, sourceIP, protocol, destIP, destPort)
		if terminated {
			break
		}
	}

	return tracer.matches, strings.Join(tracer.outputs, ", "), tracer.markMasq
}

type packetFlowTest struct {
	name     string
	sourceIP string
	protocol v1.Protocol
	destIP   string
	destPort int
	output   string
	masq     bool
}

func runPacketFlowTests(t *testing.T, line string, nft *knftables.Fake, nodeIPs []string, testCases []packetFlowTest) {
	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			protocol := strings.ToLower(string(tc.protocol))
			if protocol == "" {
				protocol = "tcp"
			}
			matches, output, masq := tracePacket(t, nft, tc.sourceIP, protocol, tc.destIP, fmt.Sprintf("%d", tc.destPort), nodeIPs)
			var errors []string
			if output != tc.output {
				errors = append(errors, fmt.Sprintf("wrong output: expected %q got %q", tc.output, output))
			}
			if masq != tc.masq {
				errors = append(errors, fmt.Sprintf("wrong masq: expected %v got %v", tc.masq, masq))
			}
			if errors != nil {
				t.Errorf("Test %q of a packet from %s to %s:%d%s got result:\n%s\n\nBy matching:\n%s\n\n",
					tc.name, tc.sourceIP, tc.destIP, tc.destPort, line, strings.Join(errors, "\n"), strings.Join(matches, "\n"))
			}
		})
	}
}

// helpers_test unit tests

var testInput = dedent.Dedent(`
	add table ip testing { comment "rules for kube-proxy" ; }

	add chain ip testing forward
	add rule ip testing forward ct state invalid drop
	add chain ip testing mark-for-masquerade
	add rule ip testing mark-for-masquerade mark set mark or 0x4000
	add chain ip testing masquerading
	add rule ip testing masquerading mark and 0x4000 == 0 return
	add rule ip testing masquerading mark set mark xor 0x4000
	add rule ip testing masquerading masquerade fully-random

	add set ip testing firewall { type ipv4_addr . inet_proto . inet_service ; comment "destinations that are subject to LoadBalancerSourceRanges" ; }
	add set ip testing firewall-allow { type ipv4_addr . inet_proto . inet_service . ipv4_addr ; flags interval ; comment "destinations+sources that are allowed by LoadBalancerSourceRanges" ; }
	add chain ip testing firewall-check
	add chain ip testing firewall-allow-check
	add rule ip testing firewall-allow-check ip daddr . meta l4proto . th dport . ip saddr @firewall-allow return
	add rule ip testing firewall-allow-check drop
	add rule ip testing firewall-check ip daddr . meta l4proto . th dport @firewall jump firewall-allow-check

	# svc1
	add chain ip testing service-ULMVA6XW-ns1/svc1/tcp/p80
	add rule ip testing service-ULMVA6XW-ns1/svc1/tcp/p80 ip daddr 172.30.0.41 tcp dport 80 ip saddr != 10.0.0.0/8 jump mark-for-masquerade
	add rule ip testing service-ULMVA6XW-ns1/svc1/tcp/p80 numgen random mod 1 vmap { 0 : goto endpoint-5OJB2KTY-ns1/svc1/tcp/p80__10.180.0.1/80 }

	add chain ip testing endpoint-5OJB2KTY-ns1/svc1/tcp/p80__10.180.0.1/80
	add rule ip testing endpoint-5OJB2KTY-ns1/svc1/tcp/p80__10.180.0.1/80 ip saddr 10.180.0.1 jump mark-for-masquerade
	add rule ip testing endpoint-5OJB2KTY-ns1/svc1/tcp/p80__10.180.0.1/80 meta l4proto tcp dnat to 10.180.0.1:80

	add element ip testing service-ips { 172.30.0.41 . tcp . 80 : goto service-ULMVA6XW-ns1/svc1/tcp/p80 }

	# svc2
	add chain ip testing service-42NFTM6N-ns2/svc2/tcp/p80
	add rule ip testing service-42NFTM6N-ns2/svc2/tcp/p80 ip daddr 172.30.0.42 tcp dport 80 ip saddr != 10.0.0.0/8 jump mark-for-masquerade
	add rule ip testing service-42NFTM6N-ns2/svc2/tcp/p80 numgen random mod 1 vmap { 0 : goto endpoint-SGOXE6O3-ns2/svc2/tcp/p80__10.180.0.2/80 }
	add chain ip testing external-42NFTM6N-ns2/svc2/tcp/p80
	add rule ip testing external-42NFTM6N-ns2/svc2/tcp/p80 ip saddr 10.0.0.0/8 goto service-42NFTM6N-ns2/svc2/tcp/p80 comment "short-circuit pod traffic"
	add rule ip testing external-42NFTM6N-ns2/svc2/tcp/p80 fib saddr type local jump mark-for-masquerade comment "masquerade local traffic"
	add rule ip testing external-42NFTM6N-ns2/svc2/tcp/p80 fib saddr type local goto service-42NFTM6N-ns2/svc2/tcp/p80 comment "short-circuit local traffic"
	add chain ip testing endpoint-SGOXE6O3-ns2/svc2/tcp/p80__10.180.0.2/80
	add rule ip testing endpoint-SGOXE6O3-ns2/svc2/tcp/p80__10.180.0.2/80 ip saddr 10.180.0.2 jump mark-for-masquerade
	add rule ip testing endpoint-SGOXE6O3-ns2/svc2/tcp/p80__10.180.0.2/80 meta l4proto tcp dnat to 10.180.0.2:80

	add element ip testing service-ips { 172.30.0.42 . tcp . 80 : goto service-42NFTM6N-ns2/svc2/tcp/p80 }
	add element ip testing service-ips { 192.168.99.22 . tcp . 80 : goto external-42NFTM6N-ns2/svc2/tcp/p80 }
	add element ip testing service-ips { 1.2.3.4 . tcp . 80 : goto external-42NFTM6N-ns2/svc2/tcp/p80 }
	add element ip testing service-nodeports { tcp . 3001 : goto external-42NFTM6N-ns2/svc2/tcp/p80 }

	add element ip testing no-endpoint-nodeports { tcp . 3001 comment "ns2/svc2:p80" : drop }
	add element ip testing no-endpoint-services { 1.2.3.4 . tcp . 80 comment "ns2/svc2:p80" : drop }
	add element ip testing no-endpoint-services { 192.168.99.22 . tcp . 80 comment "ns2/svc2:p80" : drop }
	`)

var testExpected = dedent.Dedent(`
	add table ip testing { comment "rules for kube-proxy" ; }
	add chain ip testing endpoint-5OJB2KTY-ns1/svc1/tcp/p80__10.180.0.1/80
	add chain ip testing endpoint-SGOXE6O3-ns2/svc2/tcp/p80__10.180.0.2/80
	add chain ip testing external-42NFTM6N-ns2/svc2/tcp/p80
	add chain ip testing firewall-allow-check
	add chain ip testing firewall-check
	add chain ip testing forward
	add chain ip testing mark-for-masquerade
	add chain ip testing masquerading
	add chain ip testing service-42NFTM6N-ns2/svc2/tcp/p80
	add chain ip testing service-ULMVA6XW-ns1/svc1/tcp/p80
	add rule ip testing endpoint-5OJB2KTY-ns1/svc1/tcp/p80__10.180.0.1/80 ip saddr 10.180.0.1 jump mark-for-masquerade
	add rule ip testing endpoint-5OJB2KTY-ns1/svc1/tcp/p80__10.180.0.1/80 meta l4proto tcp dnat to 10.180.0.1:80
	add rule ip testing endpoint-SGOXE6O3-ns2/svc2/tcp/p80__10.180.0.2/80 ip saddr 10.180.0.2 jump mark-for-masquerade
	add rule ip testing endpoint-SGOXE6O3-ns2/svc2/tcp/p80__10.180.0.2/80 meta l4proto tcp dnat to 10.180.0.2:80
	add rule ip testing external-42NFTM6N-ns2/svc2/tcp/p80 ip saddr 10.0.0.0/8 goto service-42NFTM6N-ns2/svc2/tcp/p80 comment "short-circuit pod traffic"
	add rule ip testing external-42NFTM6N-ns2/svc2/tcp/p80 fib saddr type local jump mark-for-masquerade comment "masquerade local traffic"
	add rule ip testing external-42NFTM6N-ns2/svc2/tcp/p80 fib saddr type local goto service-42NFTM6N-ns2/svc2/tcp/p80 comment "short-circuit local traffic"
	add rule ip testing firewall-allow-check ip daddr . meta l4proto . th dport . ip saddr @firewall-allow return
	add rule ip testing firewall-allow-check drop
	add rule ip testing firewall-check ip daddr . meta l4proto . th dport @firewall jump firewall-allow-check
	add rule ip testing forward ct state invalid drop
	add rule ip testing mark-for-masquerade mark set mark or 0x4000
	add rule ip testing masquerading mark and 0x4000 == 0 return
	add rule ip testing masquerading mark set mark xor 0x4000
	add rule ip testing masquerading masquerade fully-random
	add rule ip testing service-42NFTM6N-ns2/svc2/tcp/p80 ip daddr 172.30.0.42 tcp dport 80 ip saddr != 10.0.0.0/8 jump mark-for-masquerade
	add rule ip testing service-42NFTM6N-ns2/svc2/tcp/p80 numgen random mod 1 vmap { 0 : goto endpoint-SGOXE6O3-ns2/svc2/tcp/p80__10.180.0.2/80 }
	add rule ip testing service-ULMVA6XW-ns1/svc1/tcp/p80 ip daddr 172.30.0.41 tcp dport 80 ip saddr != 10.0.0.0/8 jump mark-for-masquerade
	add rule ip testing service-ULMVA6XW-ns1/svc1/tcp/p80 numgen random mod 1 vmap { 0 : goto endpoint-5OJB2KTY-ns1/svc1/tcp/p80__10.180.0.1/80 }
	add set ip testing firewall { type ipv4_addr . inet_proto . inet_service ; comment "destinations that are subject to LoadBalancerSourceRanges" ; }
	add set ip testing firewall-allow { type ipv4_addr . inet_proto . inet_service . ipv4_addr ; flags interval ; comment "destinations+sources that are allowed by LoadBalancerSourceRanges" ; }
	add element ip testing no-endpoint-nodeports { tcp . 3001 comment "ns2/svc2:p80" : drop }
	add element ip testing no-endpoint-services { 1.2.3.4 . tcp . 80 comment "ns2/svc2:p80" : drop }
	add element ip testing no-endpoint-services { 192.168.99.22 . tcp . 80 comment "ns2/svc2:p80" : drop }
	add element ip testing service-ips { 1.2.3.4 . tcp . 80 : goto external-42NFTM6N-ns2/svc2/tcp/p80 }
	add element ip testing service-ips { 172.30.0.41 . tcp . 80 : goto service-ULMVA6XW-ns1/svc1/tcp/p80 }
	add element ip testing service-ips { 172.30.0.42 . tcp . 80 : goto service-42NFTM6N-ns2/svc2/tcp/p80 }
	add element ip testing service-ips { 192.168.99.22 . tcp . 80 : goto external-42NFTM6N-ns2/svc2/tcp/p80 }
	add element ip testing service-nodeports { tcp . 3001 : goto external-42NFTM6N-ns2/svc2/tcp/p80 }
	`)

func Test_sortNFTablesTransaction(t *testing.T) {
	output := sortNFTablesTransaction(testInput)
	expected := strings.TrimSpace(testExpected)

	diff := cmp.Diff(expected, output)
	if diff != "" {
		t.Errorf("output does not match expected:\n%s", diff)
	}
}

func Test_diffNFTablesTransaction(t *testing.T) {
	diff := diffNFTablesTransaction(testInput, testExpected)
	if diff != "" {
		t.Errorf("found diff in inputs that should have been equal:\n%s", diff)
	}

	notExpected := strings.Join(strings.Split(testExpected, "\n")[2:], "\n")
	diff = diffNFTablesTransaction(testInput, notExpected)
	if diff == "" {
		t.Errorf("found no diff in inputs that should have been different")
	}
}

func Test_diffNFTablesChain(t *testing.T) {
	fake := knftables.NewFake(knftables.IPv4Family, "testing")
	tx := fake.NewTransaction()

	tx.Add(&knftables.Table{})
	tx.Add(&knftables.Chain{
		Name: "mark-masq-chain",
	})
	tx.Add(&knftables.Chain{
		Name: "masquerade-chain",
	})
	tx.Add(&knftables.Chain{
		Name: "empty-chain",
	})

	tx.Add(&knftables.Rule{
		Chain: "mark-masq-chain",
		Rule:  "mark set mark or 0x4000",
	})

	tx.Add(&knftables.Rule{
		Chain: "masquerade-chain",
		Rule:  "mark and 0x4000 == 0 return",
	})
	tx.Add(&knftables.Rule{
		Chain: "masquerade-chain",
		Rule:  "mark set mark xor 0x4000",
	})
	tx.Add(&knftables.Rule{
		Chain: "masquerade-chain",
		Rule:  "masquerade fully-random",
	})

	err := fake.Run(context.Background(), tx)
	if err != nil {
		t.Fatalf("Unexpected error running transaction: %v", err)
	}

	diff := diffNFTablesChain(fake, "mark-masq-chain", "mark set mark or 0x4000")
	if diff != "" {
		t.Errorf("unexpected difference in mark-masq-chain:\n%s", diff)
	}
	diff = diffNFTablesChain(fake, "mark-masq-chain", "mark set mark or 0x4000\n")
	if diff != "" {
		t.Errorf("unexpected difference in mark-masq-chain with trailing newline:\n%s", diff)
	}

	diff = diffNFTablesChain(fake, "masquerade-chain", "mark and 0x4000 == 0 return\nmark set mark xor 0x4000\nmasquerade fully-random")
	if diff != "" {
		t.Errorf("unexpected difference in masquerade-chain:\n%s", diff)
	}
	diff = diffNFTablesChain(fake, "masquerade-chain", "mark set mark xor 0x4000\nmasquerade fully-random")
	if diff == "" {
		t.Errorf("unexpected lack of difference in wrong masquerade-chain")
	}

	diff = diffNFTablesChain(fake, "empty-chain", "")
	if diff != "" {
		t.Errorf("unexpected difference in empty-chain:\n%s", diff)
	}
	diff = diffNFTablesChain(fake, "empty-chain", "\n")
	if diff != "" {
		t.Errorf("unexpected difference in empty-chain with trailing newline:\n%s", diff)
	}
}
