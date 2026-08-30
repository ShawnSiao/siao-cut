ALTER TABLE export_jobs
ADD COLUMN subtitle_delivery TEXT NOT NULL DEFAULT 'none';

UPDATE export_jobs
SET subtitle_delivery = 'burned'
WHERE burn_subtitles = 1;
