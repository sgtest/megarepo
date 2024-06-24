import { graphql } from 'gatsby';
import React from 'react';
import Helmet from 'react-helmet';
import { useTranslation } from 'react-i18next';

import { Container, Button } from '@freecodecamp/ui';
import Spacer from '../../components/helpers/spacer';
import FullWidthRow from '../../components/helpers/full-width-row';
import LearnLayout from '../../components/layouts/learn';
import type { MarkdownRemark, AllChallengeNode } from '../../redux/prop-types';

import './intro.css';

function IntroductionPage({
  data: { markdownRemark, allChallengeNode }
}: {
  data: {
    markdownRemark: MarkdownRemark;
    allChallengeNode: AllChallengeNode;
  };
}): React.FunctionComponentElement<typeof LearnLayout> {
  const { t } = useTranslation();
  const {
    html,
    frontmatter: { block, superBlock }
  } = markdownRemark;
  const firstLesson =
    allChallengeNode && allChallengeNode.edges[0].node.challenge;
  const firstLessonPath = firstLesson
    ? firstLesson.fields.slug
    : '/strange-place';
  const blockTitle =
    t(`intro:${superBlock}.blocks.${block}.title`) + ' | freeCodeCamp.org';
  return (
    <LearnLayout>
      <Helmet>
        <title>{blockTitle}</title>
      </Helmet>
      <Container className='intro-layout-container'>
        <FullWidthRow>
          <div
            className='intro-layout'
            dangerouslySetInnerHTML={{ __html: html }}
          />
        </FullWidthRow>
        <FullWidthRow>
          <Button block size='large' href={firstLessonPath}>
            {t('buttons.first-lesson')}
          </Button>
          <Spacer size='small' />
          <Button block size='large' href='/learn'>
            {t('buttons.view-curriculum')}
          </Button>
          <Spacer size='small' />
          <hr />
        </FullWidthRow>
      </Container>
    </LearnLayout>
  );
}

IntroductionPage.displayName = 'IntroductionPage';

export default IntroductionPage;

export const query = graphql`
  query IntroPageBySlug($slug: String!) {
    markdownRemark(fields: { slug: { eq: $slug } }) {
      frontmatter {
        block
        superBlock
      }
      html
    }
  }
`;
