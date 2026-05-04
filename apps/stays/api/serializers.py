from rest_framework import serializers
from apps.stays.models import Stay
class StaySerializer(serializers.ModelSerializer):
    class Meta: model=Stay; fields='__all__'
