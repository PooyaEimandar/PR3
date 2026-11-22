// The C interface to MLCommons LoadGen declared in pr3_loadgen.h.
// The only C++ PR3 owns (doc/19-decisions-and-risks.md, D-22).
#include "pr3_loadgen.h"

#include <array>
#include <cstdio>
#include <exception>
#include <string>
#include <vector>

#include "loadgen.h"
#include "query_sample.h"
#include "query_sample_library.h"
#include "system_under_test.h"
#include "test_settings.h"
#include "version.h"

namespace {

class Sut final : public mlperf::SystemUnderTest {
  public:
    explicit Sut(const pr3_lg_sut &sut) : sut_(sut), name_(sut.name ? sut.name : "pr3") {}

    const std::string &Name() override { return name_; }

    void IssueQuery(const std::vector<mlperf::QuerySample> &samples) override {
        // QuerySample and pr3_lg_sample have the same members; copy rather than alias.
        batch_.resize(samples.size());
        for (size_t i = 0; i < samples.size(); ++i) {
            batch_[i].id = samples[i].id;
            batch_[i].index = samples[i].index;
        }
        sut_.issue(sut_.ctx, batch_.data(), batch_.size());
    }

    void FlushQueries() override {
        if (sut_.flush) sut_.flush(sut_.ctx);
    }

  private:
    pr3_lg_sut sut_;
    std::string name_;
    std::vector<pr3_lg_sample> batch_; // LoadGen issues from one thread at a time
};

class Qsl final : public mlperf::QuerySampleLibrary {
  public:
    explicit Qsl(const pr3_lg_qsl &qsl) : qsl_(qsl), name_(qsl.name ? qsl.name : "pr3") {}

    const std::string &Name() override { return name_; }
    size_t TotalSampleCount() override { return qsl_.total_count; }
    size_t PerformanceSampleCount() override { return qsl_.performance_count; }

    void LoadSamplesToRam(const std::vector<mlperf::QuerySampleIndex> &samples) override {
        if (qsl_.load) qsl_.load(qsl_.ctx, samples.data(), samples.size());
    }

    void UnloadSamplesFromRam(const std::vector<mlperf::QuerySampleIndex> &samples) override {
        if (qsl_.unload) qsl_.unload(qsl_.ctx, samples.data(), samples.size());
    }

  private:
    pr3_lg_qsl qsl_;
    std::string name_;
};

// Converts C responses to LoadGen's type in fixed chunks, without allocating.
template <typename F> void ForEachChunk(const pr3_lg_response *responses, size_t count, F &&submit) {
    constexpr size_t kChunk = 64;
    std::array<mlperf::QuerySampleResponse, kChunk> out{};
    size_t done = 0;
    while (done < count) {
        const size_t n = (count - done < kChunk) ? count - done : kChunk;
        for (size_t i = 0; i < n; ++i) {
            const pr3_lg_response &r = responses[done + i];
            out[i] =
                mlperf::QuerySampleResponse(r.id, reinterpret_cast<uintptr_t>(r.data), r.size, r.n_tokens);
        }
        submit(out.data(), n);
        done += n;
    }
}

mlperf::TestScenario ToScenario(pr3_lg_scenario s) {
    switch (s) {
    case PR3_LG_SINGLE_STREAM: return mlperf::TestScenario::SingleStream;
    case PR3_LG_MULTI_STREAM: return mlperf::TestScenario::MultiStream;
    case PR3_LG_SERVER: return mlperf::TestScenario::Server;
    case PR3_LG_OFFLINE: return mlperf::TestScenario::Offline;
    }
    return mlperf::TestScenario::SingleStream;
}

mlperf::TestMode ToMode(pr3_lg_mode m) {
    switch (m) {
    case PR3_LG_PERFORMANCE: return mlperf::TestMode::PerformanceOnly;
    case PR3_LG_ACCURACY: return mlperf::TestMode::AccuracyOnly;
    case PR3_LG_SUBMISSION: return mlperf::TestMode::SubmissionRun;
    }
    return mlperf::TestMode::PerformanceOnly;
}

} // namespace

extern "C" const char *pr3_lg_scenario_name(pr3_lg_scenario scenario) {
    switch (scenario) {
    case PR3_LG_SINGLE_STREAM: return "SingleStream";
    case PR3_LG_MULTI_STREAM: return "MultiStream";
    case PR3_LG_SERVER: return "Server";
    case PR3_LG_OFFLINE: return "Offline";
    }
    return "SingleStream";
}

extern "C" pr3_lg_status pr3_lg_run(const pr3_lg_sut *sut, const pr3_lg_qsl *qsl, const pr3_lg_config *cfg) {
    if (!sut || !qsl || !cfg || !sut->issue || !cfg->model || !cfg->out_dir || qsl->total_count == 0 ||
        qsl->performance_count == 0 || qsl->performance_count > qsl->total_count) {
        return PR3_LG_BAD_ARGUMENT;
    }
    try {
        mlperf::TestSettings settings;
        const std::string scenario = pr3_lg_scenario_name(cfg->scenario);
        // conf_type 1 loads the embedded mlperf.conf first, then the user's file.
        const int conf_errors = cfg->user_conf ? settings.FromConfig(cfg->user_conf, cfg->model, scenario, 1)
                                               : settings.FromConfig("", cfg->model, scenario, 0);
        if (conf_errors != 0) {
            std::fprintf(stderr, "pr3_lg_run: configuration rejected (%d)\n", conf_errors);
            return PR3_LG_BAD_CONFIG;
        }
        settings.scenario = ToScenario(cfg->scenario);
        settings.mode = ToMode(cfg->mode);

        mlperf::LogSettings log;
        log.log_output.outdir = cfg->out_dir;
        log.log_output.copy_summary_to_stdout = cfg->summary_to_stdout;
        log.enable_trace = false;

        Sut s(*sut);
        Qsl q(*qsl);
        mlperf::StartTest(&s, &q, settings, log, cfg->audit_conf ? cfg->audit_conf : "audit.config");
        return PR3_LG_OK;
    } catch (const std::exception &e) {
        std::fprintf(stderr, "pr3_lg_run: %s\n", e.what());
    } catch (...) {
        std::fprintf(stderr, "pr3_lg_run: unknown failure\n");
    }
    return PR3_LG_FAILED;
}

extern "C" void pr3_lg_first_token(const pr3_lg_response *responses, size_t count) {
    ForEachChunk(responses, count,
                 [](mlperf::QuerySampleResponse *r, size_t n) { mlperf::FirstTokenComplete(r, n); });
}

extern "C" void pr3_lg_complete(const pr3_lg_response *responses, size_t count) {
    ForEachChunk(responses, count,
                 [](mlperf::QuerySampleResponse *r, size_t n) { mlperf::QuerySamplesComplete(r, n); });
}

extern "C" const char *pr3_lg_version(void) { return mlperf::LoadgenVersion().c_str(); }

extern "C" const char *pr3_lg_git_revision(void) { return mlperf::LoadgenGitRevision().c_str(); }
