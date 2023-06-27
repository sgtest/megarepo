## This is a self-documented Makefile. For usage information, run `make help`:
##
## For more information, refer to https://suva.sh/posts/well-documented-makefiles/

WIRE_TAGS = "oss"

-include local/Makefile
include .bingo/Variables.mk

.PHONY: all deps-go deps-js deps build-go build-backend build-server build-cli build-js build build-docker-full build-docker-full-ubuntu lint-go golangci-lint test-go test-js gen-ts test run run-frontend clean devenv devenv-down protobuf drone help gen-go gen-cue fix-cue

GO = go
GO_FILES ?= ./pkg/...
SH_FILES ?= $(shell find ./scripts -name *.sh)
GO_BUILD_FLAGS += $(if $(GO_BUILD_DEV),-dev)
GO_BUILD_FLAGS += $(if $(GO_BUILD_TAGS),-build-tags=$(GO_BUILD_TAGS))

targets := $(shell echo '$(sources)' | tr "," " ")

GO_INTEGRATION_TESTS := $(shell find ./pkg -type f -name '*_test.go' -exec grep -l '^func TestIntegration' '{}' '+' | grep -o '\(.*\)/' | sort -u)

all: deps build

##@ Dependencies

deps-go: ## Install backend dependencies.
	$(GO) run build.go setup

deps-js: node_modules ## Install frontend dependencies.

deps: deps-js ## Install all dependencies.

node_modules: package.json yarn.lock ## Install node modules.
	@echo "install frontend dependencies"
	YARN_ENABLE_PROGRESS_BARS=false yarn install --immutable

##@ Swagger
SPEC_TARGET = public/api-spec.json
MERGED_SPEC_TARGET := public/api-merged.json
NGALERT_SPEC_TARGET = pkg/services/ngalert/api/tooling/api.json

$(NGALERT_SPEC_TARGET):
	+$(MAKE) -C pkg/services/ngalert/api/tooling api.json

$(MERGED_SPEC_TARGET): $(SPEC_TARGET) $(NGALERT_SPEC_TARGET) $(SWAGGER) ## Merge generated and ngalert API specs
	# known conflicts DsPermissionType, AddApiKeyCommand, Json, Duration (identical models referenced by both specs)
	$(SWAGGER) mixin $(SPEC_TARGET) $(NGALERT_SPEC_TARGET) --ignore-conflicts -o $(MERGED_SPEC_TARGET)

$(SPEC_TARGET): $(SWAGGER) ## Generate API Swagger specification
	SWAGGER_GENERATE_EXTENSION=false $(SWAGGER) generate spec -m -w pkg/server -o $(SPEC_TARGET) \
	-x "github.com/grafana/grafana/pkg/services/ngalert/api/tooling/definitions" \
	-x "github.com/prometheus/alertmanager" \
	-i pkg/api/swagger_tags.json \
	--exclude-tag=alpha
	go run pkg/services/ngalert/api/tooling/cmd/clean-swagger/main.go -if $@ -of $@

swagger-api-spec: gen-go $(SPEC_TARGET) $(MERGED_SPEC_TARGET) validate-api-spec

validate-api-spec: $(MERGED_SPEC_TARGET) $(SWAGGER) ## Validate API spec
	$(SWAGGER) validate $(<)

clean-api-spec:
	rm -f $(SPEC_TARGET) $(MERGED_SPEC_TARGET) $(OAPI_SPEC_TARGET)

##@ OpenAPI 3
OAPI_SPEC_TARGET = public/openapi3.json

openapi3-gen: swagger-api-spec ## Generates OpenApi 3 specs from the Swagger 2 already generated
	$(GO) run scripts/openapi3/openapi3conv.go $(MERGED_SPEC_TARGET) $(OAPI_SPEC_TARGET)

##@ Building
gen-cue: ## Do all CUE/Thema code generation
	@echo "generate code from .cue files"
	go generate ./pkg/plugins/plugindef
	go generate ./kinds/gen.go
	go generate ./public/app/plugins/gen.go
	go generate ./pkg/kindsys/report.go

gen-go: $(WIRE)
	@echo "generate go files"
	$(WIRE) gen -tags $(WIRE_TAGS) ./pkg/server

fix-cue: $(CUE)
	@echo "formatting cue files"
	$(CUE) fix kinds/**/*.cue
	$(CUE) fix public/app/plugins/**/**/*.cue

gen-jsonnet:
	go generate ./devenv/jsonnet

build-go: gen-go ## Build all Go binaries.
	@echo "build go files"
	$(GO) run build.go $(GO_BUILD_FLAGS) build

build-backend: ## Build Grafana backend.
	@echo "build backend"
	$(GO) run build.go $(GO_BUILD_FLAGS) build-backend

build-server: ## Build Grafana server.
	@echo "build server"
	$(GO) run build.go $(GO_BUILD_FLAGS) build-server

build-cli: ## Build Grafana CLI application.
	@echo "build grafana-cli"
	$(GO) run build.go $(GO_BUILD_FLAGS) build-cli

build-js: ## Build frontend assets.
	@echo "build frontend"
	yarn run build
	yarn run plugins:build-bundled

build: build-go build-js ## Build backend and frontend.

run: $(BRA) ## Build and run web server on filesystem changes.
	$(BRA) run

run-frontend: deps-js ## Fetch js dependencies and watch frontend for rebuild
	yarn start

##@ Testing

.PHONY: test-go
test-go: test-go-unit test-go-integration

.PHONY: test-go-unit
test-go-unit: ## Run unit tests for backend with flags.
	@echo "test backend unit tests"
	$(GO) test -short -covermode=atomic -timeout=30m ./pkg/...

.PHONY: test-go-integration
test-go-integration: ## Run integration tests for backend with flags.
	@echo "test backend integration tests"
	$(GO) test -count=1 -run "^TestIntegration" -covermode=atomic -timeout=5m $(GO_INTEGRATION_TESTS)

.PHONY: test-go-integration-postgres
test-go-integration-postgres: devenv-postgres ## Run integration tests for postgres backend with flags.
	@echo "test backend integration postgres tests"
	$(GO) clean -testcache
	GRAFANA_TEST_DB=postgres \
	$(GO) test -p=1 -count=1 -run "^TestIntegration" -covermode=atomic -timeout=10m $(GO_INTEGRATION_TESTS)

.PHONY: test-go-integration-mysql
test-go-integration-mysql: devenv-mysql ## Run integration tests for mysql backend with flags.
	@echo "test backend integration mysql tests"
	GRAFANA_TEST_DB=mysql \
	$(GO) test -p=1 -count=1 -run "^TestIntegration" -covermode=atomic -timeout=10m $(GO_INTEGRATION_TESTS)

.PHONY: test-go-integration-redis
test-go-integration-redis: ## Run integration tests for redis cache.
	@echo "test backend integration redis tests"
	$(GO) clean -testcache
	REDIS_URL=localhost:6379 $(GO) test -run IntegrationRedis -covermode=atomic -timeout=2m $(GO_INTEGRATION_TESTS)

.PHONY: test-go-integration-memcached
test-go-integration-memcached: ## Run integration tests for memcached cache.
	@echo "test backend integration memcached tests"
	$(GO) clean -testcache
	MEMCACHED_HOSTS=localhost:11211 $(GO) test -run IntegrationMemcached -covermode=atomic -timeout=2m $(GO_INTEGRATION_TESTS)

test-js: ## Run tests for frontend.
	@echo "test frontend"
	yarn test

test: test-go test-js ## Run all tests.

##@ Linting
golangci-lint: $(GOLANGCI_LINT)
	@echo "lint via golangci-lint"
	$(GOLANGCI_LINT) run \
		--config .golangci.toml \
		$(GO_FILES)

lint-go: golangci-lint ## Run all code checks for backend. You can use GO_FILES to specify exact files to check

# with disabled SC1071 we are ignored some TCL,Expect `/usr/bin/env expect` scripts
shellcheck: $(SH_FILES) ## Run checks for shell scripts.
	@docker run --rm -v "$$PWD:/mnt" koalaman/shellcheck:stable \
	$(SH_FILES) -e SC1071 -e SC2162

##@ Docker

TAG_SUFFIX=$(if $(WIRE_TAGS)!=oss,-$(WIRE_TAGS))
PLATFORM=linux/amd64

build-docker-full: ## Build Docker image for development.
	@echo "build docker container"
	tar -ch . | \
	docker buildx build - \
	--platform $(PLATFORM) \
	--build-arg BINGO=false \
	--build-arg GO_BUILD_TAGS=$(GO_BUILD_TAGS) \
	--build-arg WIRE_TAGS=$(WIRE_TAGS) \
	--build-arg COMMIT_SHA=$$(git rev-parse --short HEAD) \
	--build-arg BUILD_BRANCH=$$(git rev-parse --abbrev-ref HEAD) \
	--tag grafana/grafana$(TAG_SUFFIX):dev \
	$(DOCKER_BUILD_ARGS)

build-docker-full-ubuntu: ## Build Docker image based on Ubuntu for development.
	@echo "build docker container"
	tar -ch . | \
	docker buildx build - \
	--platform $(PLATFORM) \
	--build-arg BINGO=false \
	--build-arg GO_BUILD_TAGS=$(GO_BUILD_TAGS) \
	--build-arg WIRE_TAGS=$(WIRE_TAGS) \
	--build-arg COMMIT_SHA=$$(git rev-parse --short HEAD) \
	--build-arg BUILD_BRANCH=$$(git rev-parse --abbrev-ref HEAD) \
	--build-arg BASE_IMAGE=ubuntu:20.04 \
	--build-arg GO_IMAGE=golang:1.20.4 \
	--tag grafana/grafana$(TAG_SUFFIX):dev-ubuntu \
	$(DOCKER_BUILD_ARGS)

##@ Services

# create docker-compose file with provided sources and start them
# example: make devenv sources=postgres,auth/openldap
ifeq ($(sources),)
devenv:
	@printf 'You have to define sources for this command \nexample: make devenv sources=postgres,openldap\n'
else
devenv: devenv-down ## Start optional services, e.g. postgres, prometheus, and elasticsearch.
	@cd devenv; \
	./create_docker_compose.sh $(targets) || \
	(rm -rf {docker-compose.yaml,conf.tmp,.env}; exit 1)

	@cd devenv; \
	docker-compose up -d --build
endif

devenv-down: ## Stop optional services.
	@cd devenv; \
	test -f docker-compose.yaml && \
	docker-compose down || exit 0;

devenv-postgres:
	@cd devenv; \
	sources=postgres_tests

devenv-mysql:
	@cd devenv; \
	sources=mysql_tests

##@ Helpers

# We separate the protobuf generation because most development tasks on
# Grafana do not involve changing protobuf files and protoc is not a
# go-gettable dependency and so getting it installed can be inconvenient.
#
# If you are working on changes to protobuf interfaces you may either use
# this target or run the individual scripts below directly.
protobuf: ## Compile protobuf definitions
	bash scripts/protobuf-check.sh
	bash pkg/plugins/backendplugin/pluginextensionv2/generate.sh

clean: ## Clean up intermediate build artifacts.
	@echo "cleaning"
	rm -rf node_modules
	rm -rf public/build

gen-ts:
	@echo "generating TypeScript definitions"
	go get github.com/tkrajina/typescriptify-golang-structs/typescriptify@v0.1.7
	tscriptify -interface -package=github.com/grafana/grafana/pkg/services/live/pipeline -import="import { FieldConfig } from '@grafana/data'" -target=public/app/features/live/pipeline/models.gen.ts pkg/services/live/pipeline/config.go
	go mod tidy

# This repository's configuration is protected (https://readme.drone.io/signature/).
# Use this make target to regenerate the configuration YAML files when
# you modify starlark files.
drone: $(DRONE)
	$(DRONE) starlark --format
	$(DRONE) lint .drone.yml --trusted
	$(DRONE) --server https://drone.grafana.net sign --save grafana/grafana

# Generate an Emacs tags table (https://www.gnu.org/software/emacs/manual/html_node/emacs/Tags-Tables.html) for Starlark files.
scripts/drone/TAGS: $(shell find scripts/drone -name '*.star')
	etags --lang none --regex="/def \(\w+\)[^:]+:/\1/" --regex="/\s*\(\w+\) =/\1/" $^ -o $@

format-drone:
	buildifier -r scripts/drone

help: ## Display this help.
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)
