from celery import shared_task
@shared_task
def health_task():
    return 'ok'
