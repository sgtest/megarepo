package appliance

import (
	"context"
	"fmt"
	"math"

	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/api/resource"
	"k8s.io/apimachinery/pkg/util/intstr"
	"sigs.k8s.io/controller-runtime/pkg/client"

	"github.com/sourcegraph/sourcegraph/internal/k8s/resource/container"
	"github.com/sourcegraph/sourcegraph/internal/k8s/resource/pod"
	"github.com/sourcegraph/sourcegraph/internal/k8s/resource/pvc"
	"github.com/sourcegraph/sourcegraph/internal/k8s/resource/service"
	"github.com/sourcegraph/sourcegraph/internal/k8s/resource/serviceaccount"
	"github.com/sourcegraph/sourcegraph/internal/k8s/resource/statefulset"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func (r *Reconciler) reconcileSymbols(ctx context.Context, sg *Sourcegraph, owner client.Object) error {
	if err := r.reconcileSymbolsStatefulSet(ctx, sg, owner); err != nil {
		return err
	}
	if err := r.reconcileSymbolsService(ctx, sg, owner); err != nil {
		return err
	}
	if err := r.reconcileSymbolsServiceAccount(ctx, sg, owner); err != nil {
		return err
	}
	return nil
}

func (r *Reconciler) reconcileSymbolsStatefulSet(ctx context.Context, sg *Sourcegraph, owner client.Object) error {
	name := "symbols"
	cfg := sg.Spec.Symbols
	if cfg.StorageSize == "" {
		cfg.StorageSize = "12Gi"
	}
	if cfg.Replicas == 0 {
		cfg.Replicas = 1
	}

	// TODO DRY out across all services
	storageClassName := sg.Spec.StorageClass.Name
	if storageClassName == "" {
		storageClassName = "sourcegraph"
	}

	ctr := container.NewContainer(name, cfg, corev1.ResourceRequirements{
		Requests: corev1.ResourceList{
			corev1.ResourceCPU:    resource.MustParse("500m"),
			corev1.ResourceMemory: resource.MustParse("500M"),
		},
		Limits: corev1.ResourceList{
			corev1.ResourceCPU:    resource.MustParse("2"),
			corev1.ResourceMemory: resource.MustParse("2G"),
		},
	})

	storageSize, err := resource.ParseQuantity(cfg.StorageSize)
	if err != nil {
		return errors.Wrap(err, "parsing storage size")
	}

	// Cache size is 90% of available attached storage
	cacheSize := float64(storageSize.Value()) * 0.9
	cacheSizeMB := int(math.Floor(cacheSize / 1024 / 1024))

	// TODO: https://github.com/sourcegraph/sourcegraph/issues/62076
	ctr.Image = "index.docker.io/sourcegraph/symbols:5.3.2@sha256:dd7f923bdbd5dbd231b749a7483110d40d59159084477b9fff84afaf58aad98e"

	ctr.Env = []corev1.EnvVar{
		container.NewEnvVarSecretKeyRef("REDIS_CACHE_ENDPOINT", "redis-cache", "endpoint"),
		container.NewEnvVarSecretKeyRef("REDIS_STORE_ENDPOINT", "redis-store", "endpoint"),

		{Name: "SYMBOLS_CACHE_SIZE_MB", Value: fmt.Sprintf("%d", cacheSizeMB)},

		container.NewEnvVarFieldRef("POD_NAME", "metadata.name"),
		{Name: "SYMBOLS_CACHE_DIR", Value: "/mnt/cache/$(POD_NAME)"},

		{Name: "TMPDIR", Value: "/mnt/tmp"},

		// OTEL_AGENT_HOST must be defined before OTEL_EXPORTER_OTLP_ENDPOINT to substitute the node IP on which the DaemonSet pod instance runs in the latter variable
		container.NewEnvVarFieldRef("OTEL_AGENT_HOST", "status.hostIP"),
		{Name: "OTEL_EXPORTER_OTLP_ENDPOINT", Value: "http://$(OTEL_AGENT_HOST):4317"},
	}
	ctr.Ports = []corev1.ContainerPort{
		{Name: "http", ContainerPort: 3184},
		{Name: "debug", ContainerPort: 6060},
	}
	ctr.LivenessProbe = &corev1.Probe{
		ProbeHandler: corev1.ProbeHandler{
			HTTPGet: &corev1.HTTPGetAction{
				Path:   "/healthz",
				Port:   intstr.FromString("http"),
				Scheme: corev1.URISchemeHTTP,
			},
		},
		InitialDelaySeconds: 60,
		TimeoutSeconds:      5,
	}
	ctr.ReadinessProbe = &corev1.Probe{
		ProbeHandler: corev1.ProbeHandler{
			HTTPGet: &corev1.HTTPGetAction{
				Path:   "/healthz",
				Port:   intstr.FromString("http"),
				Scheme: corev1.URISchemeHTTP,
			},
		},
		InitialDelaySeconds: 60,
		PeriodSeconds:       5,
	}
	ctr.VolumeMounts = []corev1.VolumeMount{
		{Name: "cache", MountPath: "/mnt/cache"},
		{Name: "tmp", MountPath: "/mnt/tmp"},
	}

	podTemplate := pod.NewPodTemplate(name, cfg)
	podTemplate.Template.Spec.Containers = []corev1.Container{ctr}
	podTemplate.Template.Spec.ServiceAccountName = name
	podTemplate.Template.Spec.Volumes = []corev1.Volume{
		pod.NewVolumeEmptyDir("tmp"),
	}

	sset := statefulset.NewStatefulSet(name, sg.Namespace, sg.Spec.RequestedVersion)
	sset.Spec.Template = podTemplate.Template
	sset.Spec.VolumeClaimTemplates = []corev1.PersistentVolumeClaim{
		pvc.NewPersistentVolumeClaimSpecOnly(storageSize, storageClassName),
	}

	return reconcileObject(ctx, r, sg.Spec.Symbols, &sset, &appsv1.StatefulSet{}, sg, owner)
}

func (r *Reconciler) reconcileSymbolsService(ctx context.Context, sg *Sourcegraph, owner client.Object) error {
	svc := service.NewService("symbols", sg.Namespace, sg.Spec.RepoUpdater)
	svc.Spec.Ports = []corev1.ServicePort{
		{Name: "http", TargetPort: intstr.FromString("http"), Port: 3184},
		{Name: "debug", TargetPort: intstr.FromString("debug"), Port: 6060},
	}
	svc.Spec.Selector = map[string]string{
		"app": "symbols",
	}
	return reconcileObject(ctx, r, sg.Spec.Symbols, &svc, &corev1.Service{}, sg, owner)
}

func (r *Reconciler) reconcileSymbolsServiceAccount(ctx context.Context, sg *Sourcegraph, owner client.Object) error {
	cfg := sg.Spec.Symbols
	sa := serviceaccount.NewServiceAccount("symbols", sg.Namespace, cfg)
	return reconcileObject(ctx, r, sg.Spec.Symbols, &sa, &corev1.ServiceAccount{}, sg, owner)
}
