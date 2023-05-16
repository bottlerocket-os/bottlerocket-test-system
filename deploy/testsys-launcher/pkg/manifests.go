package pkg

import (
	"bytes"
	"fmt"
	"io"
	"net/http"

	eks "github.com/aws/aws-cdk-go/awscdk/v2/awseks"
	"github.com/aws/jsii-runtime-go"
	"gopkg.in/yaml.v3"
)

const (
	// The URL of the Cert manager resources required by testsys and/or brupop
	certManagerManifestUrl string = "https://github.com/cert-manager/cert-manager/releases/download/v1.8.2/cert-manager.yaml"

	// The URL of the Brupop resources to keep the cluster up to date
	brupopManifestUrl string = "https://github.com/bottlerocket-os/bottlerocket-update-operator/releases/download/v1.1.0/bottlerocket-update-operator-v1.1.0.yaml"
)

// ApplyCertmanagerManifest applies the cert manager yaml manifests.
// Currently blocked on large manifests not being able to be loaded via Node CDK
// https://github.com/aws/aws-cdk/issues/19165
func ApplyCertmanagerManifest(cluster eks.Cluster) {
	certManagerManifests := []*map[string]interface{}{}

	res, err := http.Get(certManagerManifestUrl)
	if err != nil {
		panic(fmt.Sprintf("could not get cert-manager yaml manifest: %v", err))
	}
	defer res.Body.Close()

	body, err := io.ReadAll(res.Body)
	if err != nil {
		panic(fmt.Sprintf("could not extract body from http request: %v", err))
	}

	dec := yaml.NewDecoder(bytes.NewReader([]byte(body)))
	for {
		certManagerYamlChunk := map[string]interface{}{}
		if dec.Decode(&certManagerYamlChunk) != nil {
			// No more yamls to decode in this multi-yaml-document file.
			// So break out to apply all the yaml.
			break
		}
		certManagerManifests = append(certManagerManifests, &certManagerYamlChunk)
	}

	cluster.AddManifest(jsii.String("cert-manager-yamls"), certManagerManifests...)
}

// ApplyBrupopManifest applies the bottlerocket update operator manifests to the
// cluster to ensure it stays up to date with the latest Bottlerocket version.
// Currently blocked on large manifests not being able to be loaded via Node CDK
// https://github.com/aws/aws-cdk/issues/19165
func ApplyBrupopManifest(cluster eks.Cluster) {
	brupopManifests := []*map[string]interface{}{}

	res, err := http.Get(brupopManifestUrl)
	if err != nil {
		panic(fmt.Sprintf("could not get brupop yaml manifest: %v", err))
	}
	defer res.Body.Close()

	body, err := io.ReadAll(res.Body)
	if err != nil {
		panic(fmt.Sprintf("could not extract body from http request: %v", err))
	}

	dec := yaml.NewDecoder(bytes.NewReader([]byte(body)))
	for {
		brupopYamlChunk := map[string]interface{}{}
		if dec.Decode(&brupopYamlChunk) != nil {
			// No more yamls to decode in this multi-yaml-document file.
			// So break out to apply all the yaml.
			break
		}
		brupopManifests = append(brupopManifests, &brupopYamlChunk)
	}

	cluster.AddManifest(jsii.String("brupop-yamls"), brupopManifests...)
}
