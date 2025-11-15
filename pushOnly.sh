#!/usr/bin/env bash

#
# Copyright (c) 2023. Silatus, Inc.
# Last modified on 2/13/23, 10:31 PM
# Last modified by chris
#

# Default environment to non-prod
ENVIRONMENT="beta"

# Check if "--env=prod" is passed as an argument
for arg in "$@"
do
    case $arg in
        --env=prod)
            ENVIRONMENT="prod"
            shift # Remove --env=prod from processing
            ;;
        *)
            # Unknown option
            shift
            ;;
    esac
done

# Define the base image path
GCP_IMAGE_PATH="us-central1-docker.pkg.dev/silatus-c7c85/silatus/vecembed"

# Retrieve the latest tag from the Artifact Registry repository
CURRENT_VERSION=$(gcloud artifacts docker tags list $GCP_IMAGE_PATH --sort-by=~TAG --limit=1 --format='value(tag)' | awk '/^[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?$/' | head -n 1)

# Determine the image name based on the environment
if [[ "$ENVIRONMENT" == "prod" ]]; then
    IMAGE="silatus-vecembed-prod"
else
    IMAGE="silatus-vecembed-beta"
fi

# If env is prod, strip beta and produce just the semver
if [[ "$ENVIRONMENT" == "prod" ]]; then
    BASE_VERSION="${CURRENT_VERSION%-beta.*}"  # Remove beta suffix if present
    IFS='.' read -r major minor patch <<< "$BASE_VERSION"
    NEW_VERSION="$major.$minor.$((patch + 1))"
elif [[ $CURRENT_VERSION == *"-beta"* ]]; then
    # Extract beta version number and increment
    BETA_VERSION=$(echo "$CURRENT_VERSION" | cut -d'.' -f 4 | cut -d'-' -f 2)
    NEW_BETA_VERSION=$((BETA_VERSION + 1))

    # Extract main version without beta
    MAIN_VERSION=$(echo "$CURRENT_VERSION" | cut -d'-' -f 1)

    NEW_VERSION="$MAIN_VERSION-beta.$NEW_BETA_VERSION"
else
    # Use split version for incrementing semver
    SPLIT_VERSION=($(echo "$CURRENT_VERSION" | tr '.' '\n'))
    NEW_VERSION="${SPLIT_VERSION[0]}.${SPLIT_VERSION[1]}.$((SPLIT_VERSION[2] + 1))-beta.1"
fi

echo "Pushing version $NEW_VERSION"

sleep 2
# docker build . --platform linux/arm64 --tag silatus-app --tag $BASE_IMAGE_PATH/silatus:$NEW_VERSION
gcloud auth configure-docker us-central1-docker.pkg.dev
docker tag $IMAGE $GCP_IMAGE_PATH:$NEW_VERSION
docker push $GCP_IMAGE_PATH:$NEW_VERSION
