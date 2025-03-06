# Builder for the CUDA Sample binaries. The image we run to perform the tests
# doesn't need everything that is included in the "devel" image. So we build with
# the larger image, then copy them to the lightweight image we use to run.
FROM nvidia/cuda:12.8.0-devel-ubi9 AS builder

RUN dnf makecache \
  && dnf -y install \
    cmake \
    git-core \
  && dnf clean all \
  && rm -fr /var/cache/dnf

# Clone the samples, pinned to a version we know should work
RUN git clone --branch v12.8 --depth 1 https://github.com/NVIDIA/cuda-samples.git
RUN mkdir -p /samples

# There is a Makefile in the 'cuda-samples' project, but it will
# try to build all the samples, so we build the ones we want. There is
# a 'FILTER_OUT' variable that can be set, but as the name suggests
# it filters out what samples won't be build.
# This only includes the samples that should run on both amd64 and arm64.
RUN cd /cuda-samples/Samples/0_Introduction/vectorAdd/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/0_Introduction/simpleVoteIntrinsics/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/0_Introduction/simpleAtomicIntrinsics/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/0_Introduction/simpleAWBarrier/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/1_Utilities/deviceQuery/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/2_Concepts_and_Techniques/reductionMultiBlockCG/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/2_Concepts_and_Techniques/shfl_scan/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/3_CUDA_Features/immaTensorCoreGemm/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/3_CUDA_Features/warpAggregatedAtomicsCG/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/3_CUDA_Features/globalToShmemAsyncCopy/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/
RUN cd /cuda-samples/Samples/6_Performance/UnifiedMemoryPerf/ && cmake . && make -j$(nproc) && cp $(basename $(pwd)) /samples/

# We only need the base image to run the tests. It contains the necessary
# drivers and runtime environment we need.
FROM nvidia/cuda:12.8.0-base-ubi9
COPY ./run.sh /
COPY --from=builder /samples/* /samples/
RUN chmod +x ./run.sh
ENTRYPOINT ["./run.sh"]
