package com.shyashyashya.refit.unit.interview.dto;

import static com.shyashyashya.refit.unit.fixture.CompanyFixture.TEST_COMPANY;
import static com.shyashyashya.refit.unit.fixture.IndustryFixture.TEST_INDUSTRY;
import static com.shyashyashya.refit.unit.fixture.JobCategoryFixture.TEST_JOB_CATEGORY;
import static com.shyashyashya.refit.unit.fixture.UserFixture.TEST_USER_1;
import static org.assertj.core.api.Assertions.assertThat;

import com.shyashyashya.refit.domain.industry.model.Industry;
import com.shyashyashya.refit.domain.interview.dto.InterviewDto;
import com.shyashyashya.refit.domain.interview.model.Interview;
import com.shyashyashya.refit.domain.interview.model.InterviewResultStatus;
import com.shyashyashya.refit.domain.interview.model.InterviewReviewStatus;
import com.shyashyashya.refit.domain.interview.model.InterviewType;
import java.time.LocalDateTime;

import org.junit.jupiter.api.Test;

class InterviewDtoTest {

    @Test
    void InterviewDto_에서_industryId_와_industryName_을_정확히_반환한다() {
        // given
        Industry customIndustry = Industry.create("Healthcare");
        Interview interview = Interview.create(
                "Senior Developer",
                InterviewType.TECHNICAL,
                LocalDateTime.of(2024, 3, 15, 10, 0, 0),
                TEST_USER_1,
                TEST_COMPANY,
                customIndustry,
                TEST_JOB_CATEGORY
        );

        // when
        InterviewDto dto = InterviewDto.from(interview);

        // then
        assertThat(dto.industryId()).isEqualTo(customIndustry.getId());
        assertThat(dto.industryName()).isEqualTo("Healthcare");
    }

    @Test
    void InterviewDto_에서_industryId_와_industryName_이_NotNull_이다() {
        // given
        Industry manufacturingIndustry = Industry.create("Manufacturing");
        Interview interview = Interview.create(
                null,
                InterviewType.BEHAVIORAL,
                LocalDateTime.of(2024, 6, 20, 14, 30, 0),
                TEST_USER_1,
                TEST_COMPANY,
                manufacturingIndustry,
                TEST_JOB_CATEGORY
        );

        // when
        InterviewDto dto = InterviewDto.from(interview);

        // then
        assertThat(dto.industryId()).isNotNull();
        assertThat(dto.industryName()).isNotNull();
        assertThat(dto.industryName()).isEqualTo("Manufacturing");
    }

    @Test
    void InterviewDto_from_메서드가_모든_필드를_정확히_매핑한다() {
        // given
        Industry financeIndustry = Industry.create("Finance");
        Interview interview = Interview.create(
                "Junior Analyst",
                InterviewType.BEHAVIORAL,
                LocalDateTime.of(2024, 9, 10, 9, 0, 0),
                TEST_USER_1,
                TEST_COMPANY,
                financeIndustry,
                TEST_JOB_CATEGORY
        );

        // when
        InterviewDto dto = InterviewDto.from(interview);

        // then
        assertThat(dto.interviewId()).isEqualTo(interview.getId());
        assertThat(dto.interviewType()).isEqualTo(InterviewType.BEHAVIORAL);
        assertThat(dto.interviewResultStatus()).isEqualTo(InterviewResultStatus.WAIT);
        assertThat(dto.interviewReviewStatus()).isEqualTo(InterviewReviewStatus.NOT_LOGGED);
        assertThat(dto.companyName()).isEqualTo(TEST_COMPANY.getName());
        assertThat(dto.industryId()).isEqualTo(financeIndustry.getId());
        assertThat(dto.industryName()).isEqualTo("Finance");
        assertThat(dto.jobCategoryId()).isEqualTo(TEST_JOB_CATEGORY.getId());
        assertThat(dto.jobCategoryName()).isEqualTo(TEST_JOB_CATEGORY.getName());
    }
}
