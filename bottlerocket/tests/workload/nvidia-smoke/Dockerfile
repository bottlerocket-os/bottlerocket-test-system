# Builder for the CUDA Sample binaries. The image we run to perform the tests
# doesn't need everything that is included in the "devel" image. So we build with
# the larger image, then copy them to the lightweight image we use to run.
FROM nvidia/cuda:11.4.3-devel-ubi8 as builder

# Make sure we have git to clone the sample repo
RUN dnf makecache \
  && dnf -y install git-core \
  && dnf clean all \
  && rm -fr /var/cache/dnf

# Clone the samples, pinned to a version we know should work
RUN git clone --branch v11.6 --depth 1 https://github.com/NVIDIA/cuda-samples.git
RUN mkdir -p /samples

# There is a Makefile in the 'cuda-samples' project, but it will
# try to build all the samples, so we build the ones we want. There is
# a 'FILTER_OUT' variable that can be set, but as the name suggests
# it filters out what samples won't be build.
# This only includes the samples that should run on both amd64 and arm64.
RUN cd /cuda-samples/Samples/0_Introduction/vectorAdd/ && make && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/0_Introduction/simpleVoteIntrinsics/ && make && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/0_Introduction/simpleAtomicIntrinsics/ && make && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/0_Introduction/simpleAWBarrier/ && make && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/1_Utilities/deviceQuery/ && make && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/2_Concepts_and_Techniques/reductionMultiBlockCG/ && make && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/2_Concepts_and_Techniques/shfl_scan/ && make && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/3_CUDA_Features/immaTensorCoreGemm/ && make && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/3_CUDA_Features/warpAggregatedAtomicsCG/ && make && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/3_CUDA_Features/globalToShmemAsyncCopy/ && make && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/6_Performance/UnifiedMemoryPerf/ && make && cp $(basename $(pwd)) /samples/

# We only need the base image to run the tests. It contains the necessary
# drivers and runtime environment we need.
FROM nvidia/cuda:11.4.3-base-ubi8
COPY ./run.sh /
COPY --from=builder /samples/* /samples/
RUN chmod +x ./run.sh
ENTRYPOINT ["sh", "-c", "/run.sh"]
